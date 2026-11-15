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

struct HomeSmallView: View {
  let model: SurfaceModel
  var body: some View {
    VStack(alignment: .leading, spacing: 4) {
      HStack {
        PetImage(model: model, row: model.list.waiting.first, size: 30)
        Spacer()
        if model.mode == .needs { Text("\(model.count)").font(.system(size: 34, weight: .bold, design: .rounded)).foregroundStyle(SurfaceStyle.accent) }
      }
      Spacer(minLength: 0)
      Text(model.headline).font(.system(.subheadline, design: .rounded, weight: .semibold)).lineLimit(2).minimumScaleFactor(0.8)
      Text(model.computerLine).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
    }
    .opacity(model.dim ? 0.5 : 1)
  }
}

struct HomeMediumView: View {
  let model: SurfaceModel
  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      VStack(alignment: .leading, spacing: 2) {
        if model.mode == .needs { Text("\(model.count)").font(.system(size: 40, weight: .bold, design: .rounded)).foregroundStyle(SurfaceStyle.accent) }
        Text(model.headline).font(.system(.footnote, design: .rounded, weight: .semibold)).lineLimit(2)
        Spacer(minLength: 0)
        Text(model.computerLine).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
      }
      .frame(maxWidth: 110, alignment: .leading)
      VStack(alignment: .leading, spacing: 6) {
        ForEach(model.rows(2), id: \.id) { r in
          HStack(spacing: 8) {
            PetImage(model: model, row: r, size: 28)
            VStack(alignment: .leading, spacing: 0) {
              Text(r.name).font(.footnote.weight(.semibold)).lineLimit(1).privacySensitive()
              if !r.project.isEmpty { Text(r.project).font(.caption2).foregroundStyle(.secondary).lineLimit(1) }
            }
          }
        }
        if model.rows(2).isEmpty { Text(model.list.working > 0 ? model.text("phone.wg.working", ["n": "\(model.list.working)"]) : "").font(.footnote).foregroundStyle(.secondary) }
      }
      Spacer(minLength: 0)
    }
    .opacity(model.dim ? 0.5 : 1)
  }
}

struct HomeLargeView: View {
  let model: SurfaceModel
  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack {
        Text(model.headline).font(.system(.headline, design: .rounded)).foregroundStyle(model.mode == .needs ? SurfaceStyle.accent : .primary)
        Spacer()
        Text(model.computerLine).font(.caption).foregroundStyle(.secondary)
      }
      let shown = model.hide || model.mode == .stale ? [] : Array(model.list.rows.prefix(5))
      ForEach(shown, id: \.id) { r in
        HStack(spacing: 10) {
          PetImage(model: model, row: r, size: 32)
          VStack(alignment: .leading, spacing: 0) {
            Text(r.name).font(.subheadline.weight(.semibold)).lineLimit(1).privacySensitive()
            Text(r.project).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
          }
          Spacer()
          if r.state == "waiting" { Circle().fill(SurfaceStyle.accent).frame(width: 8, height: 8) }
        }
      }
      if shown.isEmpty && model.mode != .stale && !model.hide { Spacer() }
      Spacer(minLength: 0)
      HStack {
        if model.list.working > 0 { Text(model.text("phone.wg.working", ["n": "\(model.list.working)"])) }
        Spacer()
        if model.list.todo > 0 { Text(model.text("phone.wg.todo", ["n": "\(model.list.todo)"])) }
      }.font(.caption).foregroundStyle(.secondary)
    }
    .opacity(model.dim ? 0.5 : 1)
  }
}

// MARK: Lock Screen

struct LockInlineView: View {
  let model: SurfaceModel
  var body: some View {
    // The inline family is one line of text: the headline, short enough in both languages.
    Text(model.mode == .stale ? model.updated : "Bobix · " + model.headline)
  }
}

struct LockCircularView: View {
  let model: SurfaceModel
  var body: some View {
    ZStack {
      AccessoryWidgetBackground()
      VStack(spacing: 0) {
        Image(systemName: model.mode == .stale ? "clock" : "bell.fill").font(.system(size: 12))
        Text(model.mode == .stale ? "–" : "\(model.count)").font(.system(size: 20, weight: .bold, design: .rounded))
      }
    }
    .opacity(model.dim ? 0.5 : 1)
  }
}

struct LockRectangularView: View {
  let model: SurfaceModel
  var body: some View {
    VStack(alignment: .leading, spacing: 1) {
      Text("Bobix").font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
      Text(model.headline).font(.headline).lineLimit(1).minimumScaleFactor(0.7)
      if let r = model.rows(1).first { Text(r.name).font(.caption).lineLimit(1).privacySensitive() }
      else if model.mode == .needs { Text(model.text("phone.wg.hidden")).font(.caption).lineLimit(1) }
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
