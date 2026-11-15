import SwiftUI
import WidgetKit

@main
struct BobixWidgets: WidgetBundle {
  var body: some Widget {
    BobixHomeWidget()
    BobixLockWidget()
    BobixLiveActivity()
    BobixCountControl()
    BobixStartControl()
  }
}

// MARK: timeline

struct SurfaceEntry: TimelineEntry {
  let date: Date
  let list: SharedList
  let hide: Bool

  var model: SurfaceModel {
    SurfaceModel(list: list, hide: hide, now: date, text: .system, petDir: SharedListStore.directory())
  }

  /// What the gallery shows before there is data: made-up, and clearly sample names.
  static let sample = SurfaceEntry(
    date: Date(),
    list: SharedList(
      written: Int64(Date().timeIntervalSince1970 * 1000), computerName: "Studio Mac", computerState: "online", todo: 2, canStart: true,
      rows: [
        .init(id: "1", name: "Add a dark mode switch", project: "shop", state: "waiting", ref: "1", computer: "c"),
        .init(id: "2", name: "Fix the cart total", project: "shop", state: "working"),
      ]),
    hide: false)
}

/// Reads the shared list the app and the notification extension wrote. A reload is asked for by whoever wrote it; the
/// entries after now are the moments the list turns stale, so a surface dims on time without any reload of ours.
struct SurfaceProvider: TimelineProvider {
  func placeholder(in context: Context) -> SurfaceEntry { .sample }

  func getSnapshot(in context: Context, completion: @escaping (SurfaceEntry) -> Void) {
    completion(context.isPreview ? .sample : entry(at: Date()))
  }

  func getTimeline(in context: Context, completion: @escaping (Timeline<SurfaceEntry>) -> Void) {
    let now = Date()
    let first = entry(at: now)
    let later = first.list.staleMoments().filter { $0 > now }.map { entry(at: $0) }
    completion(Timeline(entries: [first] + later, policy: .never))
  }

  private func entry(at date: Date) -> SurfaceEntry {
    let hide = PushPaths.storePath().flatMap { try? storeHideDetails(storePath: $0) } ?? true
    return SurfaceEntry(date: date, list: SharedListStore.load(), hide: hide)
  }
}

// MARK: Home Screen and Lock Screen

struct BobixHomeWidget: Widget {
  var body: some WidgetConfiguration {
    StaticConfiguration(kind: "dev.bobix.phone.home", provider: SurfaceProvider()) { e in
      HomeWidgetView(entry: e)
        .widgetURL(e.model.url)
        .containerBackground(.fill.tertiary, for: .widget)
    }
    .configurationDisplayName(LocalizedStringKey("phone.wg.kind.home"))
    .description(LocalizedStringKey("phone.wg.kind.home.d"))
    .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
  }
}

struct HomeWidgetView: View {
  @Environment(\.widgetFamily) var family
  let entry: SurfaceEntry
  var body: some View {
    switch family {
    case .systemMedium: HomeMediumView(model: entry.model)
    case .systemLarge: HomeLargeView(model: entry.model)
    default: HomeSmallView(model: entry.model)
    }
  }
}

struct BobixLockWidget: Widget {
  var body: some WidgetConfiguration {
    StaticConfiguration(kind: "dev.bobix.phone.lock", provider: SurfaceProvider()) { e in
      LockWidgetView(entry: e)
        .widgetURL(e.model.url)
        .containerBackground(.clear, for: .widget)
    }
    .configurationDisplayName(LocalizedStringKey("phone.wg.kind.lock"))
    .description(LocalizedStringKey("phone.wg.kind.lock.d"))
    .supportedFamilies([.accessoryInline, .accessoryCircular, .accessoryRectangular])
  }
}

struct LockWidgetView: View {
  @Environment(\.widgetFamily) var family
  let entry: SurfaceEntry
  var body: some View {
    switch family {
    case .accessoryInline: LockInlineView(model: entry.model)
    case .accessoryCircular: LockCircularView(model: entry.model)
    default: LockRectangularView(model: entry.model)
    }
  }
}
