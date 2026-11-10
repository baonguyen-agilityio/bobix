# boxed.py < terminal text: exit 0 when Claude Code's prompt box (between the last two rules) still holds typed text.
import sys
L = [l.rstrip() for l in sys.stdin if l.strip()]
b = [i for i, l in enumerate(L) if l.startswith('─')]
box = ' '.join(L[b[-2] + 1:b[-1]]) if len(b) > 1 else ''
sys.exit(0 if box.startswith('❯') and box[1:].strip() else 1)
