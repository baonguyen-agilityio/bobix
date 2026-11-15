import ActivityKit
import SwiftUI
import WidgetKit

/// The Live Activity and the Dynamic Island. It draws counts and the redacted names; a tap opens the app, never answers.
struct BobixLiveActivity: Widget {
  var body: some WidgetConfiguration {
    ActivityConfiguration(for: BobixActivityAttributes.self) { context in
      ActivityBannerView(model: BobixLiveActivity.model(context.state))
        .widgetURL(BobixLiveActivity.model(context.state).url)
        .activityBackgroundTint(nil)
    } dynamicIsland: { context in
      let m = BobixLiveActivity.model(context.state)
      return DynamicIsland {
        DynamicIslandExpandedRegion(.leading) { IslandCompactLeading(model: m) }
        DynamicIslandExpandedRegion(.trailing) { IslandCount(model: m) }
        DynamicIslandExpandedRegion(.center) { Text(m.headline).font(.system(.subheadline, design: .rounded, weight: .semibold)) }
        DynamicIslandExpandedRegion(.bottom) {
          if let r = m.rows(1).first { Text(r.name).font(.footnote).lineLimit(1) } else { Text(m.computerLine).font(.caption).foregroundStyle(.secondary) }
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

  static func model(_ state: BobixActivityAttributes.ContentState, now: Date = Date()) -> SurfaceModel {
    let hide = PushPaths.storePath().flatMap { try? storeHideDetails(storePath: $0) } ?? true
    let body = state.resolved(storePath: PushPaths.storePath(), boxSecret: PushBoxKey.read()).body
    return SurfaceModel(list: body.list, hide: hide, now: now, text: .system, petDir: SharedListStore.directory())
  }
}
