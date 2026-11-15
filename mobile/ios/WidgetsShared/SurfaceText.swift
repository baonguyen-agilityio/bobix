import Foundation

/// The surfaces' strings: the `phone.wg.*` keys of src/ui/i18n.js, generated into Localizable.strings in EN and VI.
/// `{name}` placeholders are filled from `vars`. Tests give a table for each language.
struct SurfaceText {
  let lookup: (String) -> String

  static let system = SurfaceText { NSLocalizedString($0, bundle: .main, comment: "") }

  static func table(_ strings: [String: String]) -> SurfaceText { SurfaceText { strings[$0] ?? $0 } }

  func callAsFunction(_ key: String, _ vars: [String: String] = [:]) -> String {
    vars.reduce(lookup(key)) { $0.replacingOccurrences(of: "{\($1.key)}", with: $1.value) }
  }

  /// `14:05`, in the person's time zone.
  static func clock(_ date: Date, locale: Locale = .current) -> String {
    let f = DateFormatter()
    f.locale = locale
    f.dateFormat = "HH:mm"
    return f.string(from: date)
  }
}
