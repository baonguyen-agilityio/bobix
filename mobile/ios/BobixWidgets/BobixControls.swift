import AppIntents
import SwiftUI
import WidgetKit

/// Controls open the app and nothing else: the answer still needs Face ID.
private func currentModel() -> SurfaceModel {
  let hide = PushPaths.storePath().flatMap { try? storeHideDetails(storePath: $0) } ?? true
  return SurfaceModel(list: SharedListStore.load(), hide: hide, now: Date(), text: .system, petDir: nil)
}

/// "Bobix": the count, and a tap opens the first waiting request (Home when nothing waits).
struct BobixCountControl: ControlWidget {
  var body: some ControlWidgetConfiguration {
    StaticControlConfiguration(kind: "dev.bobix.phone.control.count") {
      let m = currentModel()
      ControlWidgetButton(action: OpenURLIntent(m.url)) {
        Label(m.mode == .stale ? m.updated : m.headline, systemImage: m.mode == .needs ? "bell.badge.fill" : "bell")
      }
    }
    .displayName(LocalizedStringResource("phone.wg.control.count"))
    .description(LocalizedStringResource("phone.wg.control.count.d"))
  }
}

/// "Start": opens the Start tab. The app says so when this phone has no right to start.
struct BobixStartControl: ControlWidget {
  var body: some ControlWidgetConfiguration {
    StaticControlConfiguration(kind: "dev.bobix.phone.control.start") {
      let list = SharedListStore.load()
      ControlWidgetButton(action: OpenURLIntent(URL(string: "bobix://start")!)) {
        Label(SurfaceText.system(list.canStart || list.written == 0 ? "phone.wg.control.start" : "phone.wg.nostart"), systemImage: "play.fill")
      }
    }
    .displayName(LocalizedStringResource("phone.wg.control.start"))
    .description(LocalizedStringResource("phone.wg.control.start.d"))
  }
}
