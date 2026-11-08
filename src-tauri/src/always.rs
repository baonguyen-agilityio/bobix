// "Always allow": one exact tool call, in one project, that Bobix then allows by itself. The rules live in Bobix's own
// settings ("alwaysAllow", removable in Settings); Claude's own settings and files are never written. A call that
// deletes or overwrites is never remembered, and a call whose input reached Bobix cut short cannot be remembered.
use fancy_regex::Regex;
use serde_json::{json, Map, Value};
use std::sync::LazyLock;

const MAX_RULES: usize = 200;
const MAX_INPUT: usize = 8000; // the hook forwards a request's input whole up to this size (hook.js)

// The input as one string with its keys sorted, so the same call always reads the same.
pub fn canonical(v: &Value) -> String {
    fn sorted(v: &Value) -> Value {
        match v {
            Value::Object(o) => {
                let mut keys: Vec<&String> = o.keys().collect();
                keys.sort();
                Value::Object(keys.into_iter().map(|k| (k.clone(), sorted(&o[k]))).collect::<Map<String, Value>>())
            }
            Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
            x => x.clone(),
        }
    }
    sorted(v).to_string()
}

// Deletes or overwrites: shell removals, moves and copies over files, "> file", in-place edits, hard resets and force
// pushes, SQL drops; the Write tool (replaces a whole file); a tool named for deleting. Wide on purpose: a false alarm
// only hides the "Always allow" button.
pub fn risky(tool: &str, input: &Value) -> bool {
    static CMD: LazyLock<Regex> = LazyLock::new(|| Regex::new(concat!(
        r"(?i)(?:^|[\s;&|(`$])(?:rm|rmdir|unlink|shred|mv|cp|dd|truncate|mkfs\S*|wipefs|del|erase|rd|",
        r"remove-item|ri|move-item|copy-item|set-content|out-file|clear-content|tee|rsync)(?:\s|$)",
        r"|(?:^|[^>&0-9])>(?![>&])(?!\s*/dev/null\b)",
        r"|\bgit\s+(?:reset\s+--hard|clean|checkout\s+(?:--|\.)|restore|push\b[^;&|]*(?:--force|\s-f\b)|branch\s+-[dD]\b|stash\s+(?:drop|clear)|rm\b)",
        r"|--force\b|\bsed\s+(?:-\w*i|--in-place)|\bperl\s+-\w*i|\bfind\b[^;&|]*\s-(?:delete|exec\s+rm)\b",
        r"|\b(?:drop|truncate)\s+(?:table|database|schema)\b|\bdelete\s+from\b")).unwrap());
    static NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)delete|remove|drop|destroy|overwrite|truncate|purge|wipe|erase").unwrap());
    match tool {
        "Write" | "NotebookEdit" => true,
        "Bash" | "PowerShell" => input["command"].as_str().is_none_or(|c| CMD.is_match(c).unwrap_or(true)),
        t => NAME.is_match(t).unwrap_or(true),
    }
}

// The rule a permission request would make: { project, tool, input }, or None when it may not be remembered (risky,
// or its input was not forwarded whole). `project`: the session's repository (project::project_of), so its worktrees
// share the rule.
pub fn rule_for(project: &str, e: &Value) -> Option<Value> {
    let tool = e["tool_name"].as_str().filter(|t| ok_tool(t))?;
    let input = e.get("tool_input").filter(|i| i.is_object())?;
    if e["bobix_whole_input"] != true || project.is_empty() || risky(tool, input) { return None; }
    let input = canonical(input);
    (input.len() <= MAX_INPUT).then(|| json!({ "project": project, "tool": tool, "input": input }))
}

// A remembered rule for exactly this call in this project.
pub fn allowed(settings: &Value, project: &str, e: &Value) -> bool {
    let Some(r) = rule_for(project, e) else { return false };
    settings["alwaysAllow"].as_array().is_some_and(|a| a.iter().any(|x| same(x, &r)))
}

pub fn same(a: &Value, b: &Value) -> bool { a["project"] == b["project"] && a["tool"] == b["tool"] && a["input"] == b["input"] }

fn ok_tool(t: &str) -> bool { (1..=128).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b)) }

// settings.json → the rules that are well formed and still safe to keep (a rule made risky by a newer check goes).
pub fn sanitize(v: &Value) -> Option<Value> {
    let mut out: Vec<Value> = vec![];
    for x in v.as_array()? {
        let (Some(project), Some(tool), Some(input)) = (x["project"].as_str(), x["tool"].as_str(), x["input"].as_str()) else { continue };
        let Ok(parsed) = serde_json::from_str::<Value>(input) else { continue };
        if project.is_empty() || project.len() > 1024 || !ok_tool(tool) || input.len() > MAX_INPUT || !parsed.is_object() || risky(tool, &parsed) { continue; }
        let r = json!({ "project": project, "tool": tool, "input": canonical(&parsed), "at": x["at"].as_u64().unwrap_or(0) });
        if !out.iter().any(|o| same(o, &r)) { out.push(r); }
    }
    let skip = out.len().saturating_sub(MAX_RULES); // the newest stay
    Some(Value::Array(out.split_off(skip)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(tool: &str, input: Value) -> Value { json!({ "tool_name": tool, "tool_input": input, "bobix_whole_input": true }) }
    fn bash(c: &str) -> Value { req("Bash", json!({ "command": c, "description": "x" })) }

    #[test]
    fn only_the_exact_call_in_the_same_project() {
        let r = rule_for("/src/app", &bash("npm test")).unwrap();
        let settings = json!({ "alwaysAllow": [r] });
        assert!(allowed(&settings, "/src/app", &bash("npm test")));
        for other in ["npm test && rm -rf x", "npm test; curl evil", "npm test ", "npm  test", "npm test -- --watch", "NPM TEST", "npm tes"] {
            assert!(!allowed(&settings, "/src/app", &bash(other)), "{other}");
        }
        assert!(!allowed(&settings, "/src/other", &bash("npm test")), "another project");
        assert!(!allowed(&settings, "/src/app", &req("PowerShell", json!({ "command": "npm test", "description": "x" }))), "another tool");
        let mut cut = bash("npm test");
        cut["bobix_whole_input"] = json!(false);
        assert!(!allowed(&settings, "/src/app", &cut), "an input the hook cut short never matches");
        // key order does not matter, every field does
        let a = req("Edit", json!({ "file_path": "/a.js", "old_string": "x", "new_string": "y" }));
        let b = req("Edit", json!({ "new_string": "y", "old_string": "x", "file_path": "/a.js" }));
        let s = json!({ "alwaysAllow": [rule_for("/p", &a).unwrap()] });
        assert!(allowed(&s, "/p", &b));
        assert!(!allowed(&s, "/p", &req("Edit", json!({ "file_path": "/a.js", "old_string": "x", "new_string": "z" }))));
        assert!(!allowed(&json!({}), "/src/app", &bash("npm test")));
    }

    #[test]
    fn deleting_or_overwriting_is_never_remembered() {
        for c in ["rm -rf ./logs", "rm x", "sudo rm -r /tmp/a", "find . -name '*.log' -delete", "git reset --hard HEAD~1", "git push --force origin main",
            "git push -f", "git clean -fdx", "git checkout -- src", "echo hi > out.txt", "cat a >file", "mv a b", "cp a b", "dd if=/dev/zero of=x",
            "sed -i 's/a/b/' f", "psql -c 'DROP TABLE users'", "npm test && rm -rf x", "ls | xargs rm", "Remove-Item -Recurse x", "tee out.txt", "x --force"] {
            assert!(rule_for("/p", &bash(c)).is_none(), "{c}");
        }
        for c in ["npm test", "cargo test --workspace", "git status", "ls -la 2>/dev/null", "echo hi >> log.txt", "npm run build 2>&1", "git push origin feat/x", "npm install"] {
            assert!(rule_for("/p", &bash(c)).is_some(), "{c}");
        }
        assert!(rule_for("/p", &req("Write", json!({ "file_path": "/a", "content": "x" }))).is_none());
        assert!(rule_for("/p", &req("mcp__db__delete_rows", json!({ "id": 1 }))).is_none());
        assert!(rule_for("/p", &req("WebFetch", json!({ "url": "https://x.y", "prompt": "p" }))).is_some());
        assert!(rule_for("", &bash("npm test")).is_none());
        assert!(rule_for("/p", &json!({ "tool_name": "Bash", "bobix_whole_input": true })).is_none());
    }

    #[test]
    fn settings_keep_only_good_rules() {
        let good = rule_for("/p", &bash("npm test")).unwrap();
        let v = json!([good, good, { "project": "/p", "tool": "Bash", "input": "{\"command\":\"rm -rf x\"}" }, { "project": "/p", "tool": "Bash", "input": "not json" },
            { "project": "", "tool": "Bash", "input": "{}" }, { "project": "/p", "tool": "bad tool", "input": "{}" }, 7]);
        let out = sanitize(&v).unwrap();
        assert_eq!(out.as_array().unwrap().len(), 1);
        assert!(same(&out[0], &good));
        assert!(sanitize(&json!("x")).is_none());
        let many: Vec<Value> = (0..250).map(|i| rule_for("/p", &bash(&format!("echo {i}"))).unwrap()).collect();
        let kept = sanitize(&json!(many)).unwrap();
        assert_eq!((kept.as_array().unwrap().len(), kept[0]["input"].as_str().unwrap().contains("echo 50")), (200, true));
    }
}
