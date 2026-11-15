import ActivityKit
import SwiftUI
import WidgetKit

/// The Live Activity and the Dynamic Island. It draws counts and the redacted names; a tap opens the app, never answers.
/// Content the system calls stale (the activity's stale date is its last update plus 15 minutes) is drawn stale.
struct BobixLiveActivity: Widget {
  var body: some WidgetConfiguration {
    ActivityConfiguration(for: BobixActivityAttributes.self) { context in
      let m = BobixLiveActivity.model(context.state, isStale: context.isStale)
      ActivityBannerView(model: m)
        .widgetURL(m.url)
        .activityBackgroundTint(nil)
    } dynamicIsland: { context in
      let m = BobixLiveActivity.model(context.state, isStale: context.isStale)
      return DynamicIsland {
        DynamicIslandExpandedRegion(.leading) { IslandCompactLeading(model: m) }
        DynamicIslandExpandedRegion(.trailing) { IslandCount(model: m) }
        DynamicIslandExpandedRegion(.center) { Text(m.headline).font(.system(.subheadline, design: .rounded, weight: .semibold)) }
        DynamicIslandExpandedRegion(.bottom) {
          if let r = m.rows(1).first { Text(r.name).font(.footnote).lineLimit(1).privacySensitive() } else { Text(m.computerLine).font(.caption).foregroundStyle(.secondary) }
        }
      } compactLeading: {
        IslandCompactLeading(model: m)
      } compactTrailing: {
        IslandCount(model: m)
      } minimal: {
        IslandCount(model: m)
      }
      .widgetURL(m.url)
    }
  }

  /// Built once for each layout of a draw: it reads the keychain and the shared files.
  static func model(_ state: BobixActivityAttributes.ContentState, isStale: Bool, now: Date = Date()) -> SurfaceModel {
    let store = PushPaths.storePath()
    let hide = store.flatMap { try? storeHideDetails(storePath: $0) } ?? true
    let list = state.resolved(storePath: store, boxSecret: PushBoxKey.read(), file: SharedListStore.load(), now: now).list
    var m = SurfaceModel(list: list, hide: hide, now: now, text: .system, petDir: SharedListStore.directory())
    m.forceStale = isStale
    return m
  }
}
