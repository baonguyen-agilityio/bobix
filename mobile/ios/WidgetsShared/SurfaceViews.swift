import SwiftUI
import WidgetKit

/// Everything a surface needs to draw itself, so the widget extension and the tests draw the same thing.
struct SurfaceModel {
  var list: SharedList
  /// "Hide details" on: counts only, no session or project name.
  var hide: Bool
  var now: Date
  var text: SurfaceText
  var petDir: URL?
  /// The system says the content is stale (a Live Activity's `isStale`): drawn stale whatever the list says.
  var forceStale = false

  enum Mode { case needs, none, stale }
  var mode: Mode { forceStale || list.isStale(at: now) ? .stale : (list.waiting.isEmpty ? .none : .needs) }
  var count: Int { list.waiting.count }
  var dim: Bool { mode == .stale }
  var updated: String { text("phone.wg.updated", ["tm": SurfaceText.clock(list.writtenDate)]) }

  /// What the surface says first. Old data never says "needs you".
  var headline: String {
    switch mode {
    case .stale: return updated
    case .none: return text("phone.wg.none")
    case .needs: return count == 1 ? text("phone.wg.need.1") : text("phone.wg.need.n", ["n": "\(count)"])
    }
  }

  var computerLine: String {
    switch list.computerState {
    case "asleep": return text("phone.wg.asleep")
    case "offline": return text("phone.wg.offline")
    default: return list.computerName
    }
  }

  /// Names to show: none with Hide details on, and none on old data.
  func rows(_ max: Int) -> [SharedList.Row] { hide || mode == .stale ? [] : Array(list.waiting.prefix(max)) }

  /// The one URL a surface opens: the first waiting request, else Home. Never an answer.
  var url: URL {
    if mode == .needs, let r = list.waiting.first, !r.ref.isEmpty {
      var c = URLComponents(string: "bobix://request")!
      c.queryItems = [URLQueryItem(name: "computer", value: r.computer), URLQueryItem(name: "ref", value: r.ref)]
      return c.url!
    }
    return URL(string: "bobix://home")!
  }
}

enum SurfaceStyle {
  static let accent = Color(red: 0.91, green: 0.40, blue: 0.24)
}

struct PetImage: View {
  let model: SurfaceModel
  let row: SharedList.Row?
  var size: CGFloat = 36

  var body: some View {
    let url = row.flatMap { SharedListStore.petURL(design: $0.pet, color: $0.color, state: $0.state, in: model.petDir) }
    if let url, let image = UIImage(contentsOfFile: url.path) {
      Image(uiImage: image).resizable().scaledToFit().frame(width: size, height: size)
    } else {
      Image(systemName: "pawprint.fill").font(.system(size: size * 0.6)).foregroundStyle(SurfaceStyle.accent).frame(width: size, height: size)
    }
  }
}

// MARK: Home Screen

extension SurfaceModel {
  /// The screen one row opens: its own request when the row waits and the data is fresh, else Home. Never an answer.
  func url(for row: SharedList.Row) -> URL {
    guard mode == .needs, row.state == "waiting", !row.ref.isEmpty else { return URL(string: "bobix://home")! }
    var c = URLComponents(string: "bobix://request")!
    c.queryItems = [URLQueryItem(name: "computer", value: row.computer), URLQueryItem(name: "ref", value: row.ref)]
    return c.url!
  }

  /// The pet's state colour: the small dot on its corner.
  func dot(_ state: String) -> Color {
    switch state {
    case "waiting": return SurfaceStyle.accent
    case "working": return .blue
    case "done": return .green
    case "failed": return .red
    default: return .gray
    }
  }
}

/// "Bobix" with its paw, and on the right the computer (or, when old, when it was last up to date).
struct WidgetHead: View {
  let model: SurfaceModel
  /// The small and medium widgets show no computer name.
  var short = false

  var body: some View {
    HStack(spacing: 4) {
      Image(systemName: "pawprint").font(.system(size: 11, weight: .bold))
      Text("Bobix").font(.system(size: 12, weight: .bold))
      Spacer(minLength: 4)
      if model.mode == .stale || model.list.computerState != "online" {
        Text(model.mode == .stale ? model.updated : model.computerLine).font(.system(size: 11, weight: .medium)).foregroundStyle(.secondary).lineLimit(1)
      } else if !short {
        Circle().fill(Color.green).frame(width: 6, height: 6)
        Text(model.list.computerName).font(.system(size: 11, weight: .medium)).foregroundStyle(.secondary).lineLimit(1)
      }
    }
    .foregroundStyle(SurfaceStyle.accent)
  }
}

/// The big count and what it counts.
struct WidgetBig: View {
  let model: SurfaceModel
  var body: some View {
    VStack(alignment: .leading, spacing: 2) {
      Text("\(model.count)").font(.system(size: 44, weight: .bold)).foregroundStyle(.primary).minimumScaleFactor(0.6).lineLimit(1)
      Text(model.text("phone.wg.need.label")).font(.system(size: 13, weight: .semibold)).foregroundStyle(SurfaceStyle.accent).lineLimit(1)
    }
  }
}

/// Nothing waits: a tick and one line.
struct WidgetEmpty: View {
  let model: SurfaceModel
  var body: some View {
    VStack(spacing: 6) {
      Image(systemName: "checkmark.circle").font(.system(size: 18)).foregroundStyle(Color.green)
      Text(model.mode == .stale ? model.updated : model.text("phone.wg.none")).font(.system(size: 13, weight: .semibold)).multilineTextAlignment(.center).foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
  }
}

/// A row of pets, each with the dot of its state.
struct WidgetPets: View {
  let model: SurfaceModel
  let max: Int
  var gap: CGFloat = 4
  var body: some View {
    HStack(spacing: gap) {
      ForEach(Array(model.list.rows.prefix(max)), id: \.id) { r in
        ZStack(alignment: .bottomTrailing) {
          PetImage(model: model, row: r, size: 26)
          Circle().fill(model.dot(r.state)).frame(width: 7, height: 7).overlay(Circle().stroke(Color(UIColor.systemBackground), lineWidth: 1.5))
        }
      }
    }
  }
}

/// One request that waits: its pet, name and project, and a chevron. A tap opens this request, not just the app.
struct WidgetRow: View {
  let model: SurfaceModel
  let row: SharedList.Row
  var body: some View {
    Link(destination: model.url(for: row)) {
      HStack(spacing: 8) {
        PetImage(model: model, row: row, size: 28)
        VStack(alignment: .leading, spacing: 0) {
          Text(row.name).font(.system(size: 13, weight: .semibold)).foregroundStyle(Color.primary).lineLimit(1).privacySensitive()
          if !row.project.isEmpty { Text(row.project).font(.system(size: 11)).foregroundStyle(Color.secondary).lineLimit(1) }
        }
        Spacer(minLength: 0)
        Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(Color.secondary)
      }
      .padding(.vertical, 6).padding(.leading, 6).padding(.trailing, 8)
      .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color.primary.opacity(0.05)))
    }
  }
}

struct HomeSmallView: View {
  let model: SurfaceModel
  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      WidgetHead(model: model, short: true)
      if model.mode == .needs {
        HStack(alignment: .top) {
          WidgetBig(model: model)
          Spacer(minLength: 0)
          PetImage(model: model, row: model.list.waiting.first, size: 34)
        }
        Spacer(minLength: 0)
        if let r = model.rows(1).first { Text(r.name).font(.system(size: 13, weight: .semibold)).lineLimit(2).privacySensitive() }
      } else {
        WidgetEmpty(model: model)
      }
    }
    .opacity(model.dim ? 0.55 : 1)
  }
}

struct HomeMediumView: View {
  let model: SurfaceModel
  var body: some View {
    HStack(alignment: .top, spacing: 14) {
      VStack(alignment: .leading, spacing: 6) {
        WidgetHead(model: model, short: true)
        if model.mode == .needs { WidgetBig(model: model) }
        Spacer(minLength: 0)
        if !model.hide { WidgetPets(model: model, max: 4) }
      }
      .frame(width: 112, alignment: .leading)
      VStack(spacing: 6) {
        if model.mode == .needs && !model.hide {
          ForEach(model.rows(2), id: \.id) { WidgetRow(model: model, row: $0) }
        } else if model.mode == .needs {
          Text(model.text("phone.wg.hidden")).font(.system(size: 13)).foregroundStyle(.secondary).frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
          WidgetEmpty(model: model)
        }
      }
      .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
    .opacity(model.dim ? 0.55 : 1)
  }
}

struct HomeLargeView: View {
  let model: SurfaceModel
  private func section(_ key: String, _ n: Int?) -> some View {
    HStack(spacing: 4) {
      Text(model.text(key).uppercased())
      if let n { Text("\(n)").foregroundStyle(SurfaceStyle.accent) }
    }
    .font(.system(size: 11, weight: .bold)).foregroundStyle(.secondary).tracking(0.4)
  }
  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      WidgetHead(model: model)
      section("phone.wg.sec.need", model.mode == .needs ? model.count : 0)
      if model.mode == .needs && !model.hide {
        ForEach(model.rows(2), id: \.id) { WidgetRow(model: model, row: $0) }
      } else if model.mode == .needs {
        Text(model.text("phone.wg.hidden")).font(.system(size: 13)).foregroundStyle(.secondary)
      } else {
        WidgetEmpty(model: model).frame(height: 70)
      }
      if !model.hide && model.mode != .stale && !model.list.rows.isEmpty {
        section("phone.wg.sec.pets", nil)
        WidgetPets(model: model, max: 6, gap: 10)
      }
      if model.list.todo > 0 && model.mode != .stale {
        section("phone.wg.sec.todo", nil)
        Link(destination: URL(string: model.list.canStart ? "bobix://start" : "bobix://home")!) {
          HStack(spacing: 8) {
            Image(systemName: "ticket").font(.system(size: 15)).foregroundStyle(Color.secondary).frame(width: 28, height: 28)
              .background(RoundedRectangle(cornerRadius: 8, style: .continuous).fill(Color.primary.opacity(0.07)))
            Text(model.text("phone.wg.todo", ["n": "\(model.list.todo)"])).font(.system(size: 13, weight: .semibold)).foregroundStyle(Color.primary).lineLimit(1)
            Spacer(minLength: 0)
            Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(Color.secondary)
          }
          .padding(.vertical, 4).padding(.leading, 5).padding(.trailing, 8)
          .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color.primary.opacity(0.05)))
        }
      }
      Spacer(minLength: 0)
    }
    .opacity(model.dim ? 0.55 : 1)
  }
}

// MARK: Lock Screen

extension SurfaceModel {
  /// The inline Lock Screen line: the count and the computer, or the age of old data. With Hide details on, counts only.
  var inlineLine: String {
    let who = hide ? "" : " · \(list.computerName)"
    switch mode {
    case .stale: return updated
    case .needs: return "\(count) \(text("phone.wg.need.label"))\(who)"
    case .none: return "\(text("phone.wg.none"))\(who)"
    }
  }
}

struct LockInlineView: View {
  let model: SurfaceModel
  var body: some View {
    HStack(spacing: 4) {
      Image(systemName: "pawprint")
      Text(model.inlineLine)
    }
  }
}

struct LockCircularView: View {
  let model: SurfaceModel
  var body: some View {
    let fraction = model.mode == .stale ? 0 : min(Double(model.count) / 5, 1)
    ZStack {
      Circle().stroke(Color.primary.opacity(0.25), lineWidth: 3)
      Circle().trim(from: 0, to: fraction).stroke(Color.primary, style: StrokeStyle(lineWidth: 3, lineCap: .round)).rotationEffect(.degrees(-90))
      VStack(spacing: 0) {
        Text(model.mode == .stale ? "–" : "\(model.count)").font(.system(size: 20, weight: .bold, design: .rounded))
        Image(systemName: model.mode == .stale ? "clock" : "pawprint").font(.system(size: 9))
      }
    }
    .padding(4)
    .opacity(model.dim ? 0.5 : 1)
  }
}

struct LockRectangularView: View {
  let model: SurfaceModel
  var body: some View {
    VStack(alignment: .leading, spacing: 1) {
      HStack(spacing: 4) {
        Image(systemName: "pawprint").font(.system(size: 11))
        Text(model.mode == .needs ? "\(model.count) \(model.text("phone.wg.need.label"))" : "Bobix").font(.system(size: 11, weight: .bold))
      }.opacity(0.85)
      if let r = model.rows(1).first {
        Text(r.name).font(.system(size: 14, weight: .semibold)).lineLimit(1).privacySensitive()
        Text(r.project.isEmpty ? model.list.computerName : "\(r.project) · \(model.list.computerName)").font(.system(size: 11)).opacity(0.75).lineLimit(1).privacySensitive()
      } else if model.mode == .needs {
        Text(model.text("phone.wg.hidden")).font(.system(size: 13, weight: .semibold)).lineLimit(1)
      } else {
        Text(model.mode == .stale ? model.updated : model.text("phone.wg.none")).font(.system(size: 14, weight: .semibold)).lineLimit(1)
        if !model.hide { Text(model.list.computerName).font(.system(size: 11)).opacity(0.75).lineLimit(1) }
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .opacity(model.dim ? 0.5 : 1)
  }
}

// MARK: Live Activity and Dynamic Island

/// What the activity draws. The same view serves the Lock Screen banner and the expanded island.
struct ActivityBannerView: View {
  let model: SurfaceModel
  var body: some View {
    HStack(spacing: 12) {
      PetImage(model: model, row: model.list.waiting.first, size: 40)
      VStack(alignment: .leading, spacing: 2) {
        Text(model.headline).font(.system(.headline, design: .rounded)).foregroundStyle(model.mode == .needs ? SurfaceStyle.accent : .primary)
        if let r = model.rows(1).first { Text(r.name).font(.subheadline).lineLimit(1).privacySensitive() }
        Text(model.computerLine).font(.caption2).foregroundStyle(.secondary)
      }
      Spacer()
    }
    .padding(14)
    .opacity(model.dim ? 0.5 : 1)
  }
}

struct IslandCompactLeading: View {
  let model: SurfaceModel
  var body: some View { PetImage(model: model, row: model.list.waiting.first, size: 20) }
}

struct IslandCount: View {
  let model: SurfaceModel
  var body: some View {
    Text(model.mode == .stale ? "–" : "\(model.count)").font(.system(.body, design: .rounded, weight: .bold)).foregroundStyle(SurfaceStyle.accent)
  }
}
