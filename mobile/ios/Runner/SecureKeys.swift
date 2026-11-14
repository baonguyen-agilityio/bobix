import Flutter
import Foundation
import LocalAuthentication
import Security

/// The three keys of the app.
///
/// - `auth` and `box` are secrets the Rust core makes; they are kept as Keychain items that are readable after the first
///   unlock, never leave this device (no iCloud Keychain, no backup) and need no biometrics.
/// - `sign` is a P-256 key made in the Secure Enclave. It cannot be exported, asks for the enrolled biometrics on every
///   signature, stops working when the enrolled biometrics change, and has no passcode fallback. The app only ever gets
///   signatures and the public key.
///
/// A Keychain item outlives the app on iOS, so a fresh install (the marker file is missing) erases every key first.
final class KeyStore {
  enum SignOutcome: Equatable {
    case signed(Data)
    case cancelled
    /// The key is gone or unusable (biometrics changed, passcode removed): the phone must pair again.
    case invalidated
    case failed(String)
  }

  struct KeyError: Error { let message: String }

  private let service: String
  private let signTag: Data
  private let markerURL: URL
  /// A plain Keychain key in place of the Secure Enclave one. The simulator cannot make a Secure Enclave key with
  /// biometric access control, so the tests use this; it exists only in debug builds and never ships.
  private let software: Bool

  init(service: String = "dev.bobix.phone.keys", software: Bool = false, markerURL: URL? = nil) throws {
    #if !DEBUG
    if software { throw KeyError(message: "the software key is not available in a release build") }
    #endif
    self.service = service
    self.signTag = Data("\(service).sign".utf8)
    self.software = software
    self.markerURL = try markerURL ?? KeyStore.defaultMarkerURL()
    try eraseIfFreshInstall()
  }

  private static func defaultMarkerURL() throws -> URL {
    var dir = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
    // The app's data stays out of device and cloud backups.
    var values = URLResourceValues()
    values.isExcludedFromBackup = true
    try? dir.setResourceValues(values)
    return dir.appendingPathComponent("keys-installed")
  }

  // MARK: install marker

  private func eraseIfFreshInstall() throws {
    if FileManager.default.fileExists(atPath: markerURL.path) { return }
    eraseAll()
    try FileManager.default.createDirectory(at: markerURL.deletingLastPathComponent(), withIntermediateDirectories: true)
    guard FileManager.default.createFile(atPath: markerURL.path, contents: Data()) else {
      throw KeyError(message: "could not write the install marker")
    }
    var url = markerURL
    var values = URLResourceValues()
    values.isExcludedFromBackup = true
    try? url.setResourceValues(values)
  }

  // MARK: auth and box

  private func secretQuery(_ name: String) -> [String: Any] {
    [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: name,
      kSecAttrSynchronizable as String: false,
    ]
  }

  func putSecret(_ name: String, _ data: Data) throws {
    var add = secretQuery(name)
    add[kSecValueData as String] = data
    add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    var status = SecItemAdd(add as CFDictionary, nil)
    if status == errSecDuplicateItem {
      status = SecItemUpdate(secretQuery(name) as CFDictionary, [kSecValueData as String: data] as CFDictionary)
    }
    if status != errSecSuccess { throw KeyError(message: "keychain write failed (\(status))") }
  }

  func secret(_ name: String) -> Data? {
    var query = secretQuery(name)
    query[kSecReturnData as String] = true
    query[kSecMatchLimit as String] = kSecMatchLimitOne
    var out: CFTypeRef?
    return SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess ? out as? Data : nil
  }

  func deleteSecret(_ name: String) {
    SecItemDelete(secretQuery(name) as CFDictionary)
  }

  // MARK: sign

  private var keyQuery: [String: Any] {
    [
      kSecClass as String: kSecClassKey,
      kSecAttrApplicationTag as String: signTag,
      kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
    ]
  }

  /// Makes the signing key (replacing any earlier one) and returns its public key in X9.63 form.
  func createSigningKey() throws -> Data {
    SecItemDelete(keyQuery as CFDictionary)
    var attributes: [String: Any] = [
      kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
      kSecAttrKeySizeInBits as String: 256,
    ]
    var privateAttributes: [String: Any] = [kSecAttrIsPermanent as String: true, kSecAttrApplicationTag as String: signTag]
    if software {
      privateAttributes[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    } else {
      var error: Unmanaged<CFError>?
      // Biometrics only (no passcode fallback), the key dies when the enrolled biometrics change, never leaves the chip.
      guard let access = SecAccessControlCreateWithFlags(
        nil, kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly, [.privateKeyUsage, .biometryCurrentSet], &error)
      else { throw KeyError(message: "access control: \(error.map { String(describing: $0.takeRetainedValue()) } ?? "unknown")") }
      attributes[kSecAttrTokenID as String] = kSecAttrTokenIDSecureEnclave
      privateAttributes[kSecAttrAccessControl as String] = access
    }
    attributes[kSecPrivateKeyAttrs as String] = privateAttributes

    var error: Unmanaged<CFError>?
    guard let key = SecKeyCreateRandomKey(attributes as CFDictionary, &error) else {
      throw KeyError(message: "signing key: \(error.map { String(describing: $0.takeRetainedValue()) } ?? "unknown")")
    }
    return try exportPublic(key)
  }

  private func exportPublic(_ privateKey: SecKey) throws -> Data {
    var error: Unmanaged<CFError>?
    guard let pub = SecKeyCopyPublicKey(privateKey), let data = SecKeyCopyExternalRepresentation(pub, &error) as Data? else {
      throw KeyError(message: "public key: \(error.map { String(describing: $0.takeRetainedValue()) } ?? "unknown")")
    }
    return data
  }

  func signingPublicKey() -> Data? {
    var query = keyQuery
    query[kSecReturnRef as String] = true
    // Reading the public half must never ask for biometrics.
    let context = LAContext()
    context.interactionNotAllowed = true
    query[kSecUseAuthenticationContext as String] = context
    var out: CFTypeRef?
    guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess, let ref = out else { return nil }
    return try? exportPublic(ref as! SecKey)
  }

  /// Signs [data] (ECDSA P-256 over SHA-256, X9.62 DER). The Secure Enclave key asks for Face ID or the fingerprint here.
  func sign(_ data: Data, reason: String) -> SignOutcome {
    let context = LAContext()
    context.localizedReason = reason
    context.localizedFallbackTitle = "" // no passcode button
    var query = keyQuery
    query[kSecReturnRef as String] = true
    query[kSecUseAuthenticationContext as String] = context
    var out: CFTypeRef?
    let found = SecItemCopyMatching(query as CFDictionary, &out)
    if found == errSecItemNotFound { return .invalidated }
    guard found == errSecSuccess, let ref = out else { return outcome(forStatus: found) }

    var error: Unmanaged<CFError>?
    guard let signature = SecKeyCreateSignature(ref as! SecKey, .ecdsaSignatureMessageX962SHA256, data as CFData, &error) as Data? else {
      return outcome(for: error?.takeRetainedValue())
    }
    return .signed(signature)
  }

  private func outcome(forStatus status: OSStatus) -> SignOutcome {
    switch status {
    case errSecUserCanceled: return .cancelled
    case errSecAuthFailed, errSecItemNotFound: return .invalidated
    default: return .failed("keychain (\(status))")
    }
  }

  private func outcome(for error: CFError?) -> SignOutcome {
    guard let error else { return .failed("unknown") }
    let ns = error as Error as NSError
    if ns.domain == LAError.errorDomain {
      switch LAError.Code(rawValue: ns.code) {
      case .userCancel, .systemCancel, .appCancel: return .cancelled
      case .biometryNotEnrolled, .biometryLockout, .biometryNotAvailable: return .invalidated
      default: break
      }
    }
    if ns.domain == NSOSStatusErrorDomain { return outcome(forStatus: OSStatus(ns.code)) }
    return .failed(ns.localizedDescription)
  }

  func deleteSigningKey() {
    SecItemDelete(keyQuery as CFDictionary)
  }

  func eraseAll() {
    SecItemDelete([kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrSynchronizable as String: kSecAttrSynchronizableAny] as CFDictionary)
    deleteSigningKey()
  }
}

/// The platform channel `dev.bobix.phone/secure_keys`: the Dart side of the key interface (see lib/keys).
final class SecureKeysChannel {
  private var store: KeyStore?
  private let channel: FlutterMethodChannel

  init(messenger: FlutterBinaryMessenger) {
    channel = FlutterMethodChannel(name: "dev.bobix.phone/secure_keys", binaryMessenger: messenger)
    channel.setMethodCallHandler { [weak self] call, result in self?.handle(call, result) }
  }

  private func handle(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
    let args = call.arguments as? [String: Any] ?? [:]
    if call.method == "open" {
      do {
        store = try KeyStore(software: args["software"] as? Bool ?? false)
        result(nil)
      } catch let e as KeyStore.KeyError {
        result(FlutterError(code: "open_failed", message: e.message, details: nil))
      } catch {
        result(FlutterError(code: "open_failed", message: error.localizedDescription, details: nil))
      }
      return
    }
    guard let store else { return result(FlutterError(code: "not_open", message: "call open first", details: nil)) }
    func name() -> String { args["name"] as? String ?? "" }
    func bytes(_ key: String) -> Data { (args[key] as? FlutterStandardTypedData)?.data ?? Data() }
    do {
      switch call.method {
      case "putSecret": try store.putSecret(name(), bytes("data")); result(nil)
      case "secret": result(store.secret(name()).map { FlutterStandardTypedData(bytes: $0) })
      case "deleteSecret": store.deleteSecret(name()); result(nil)
      case "createSigningKey": result(FlutterStandardTypedData(bytes: try store.createSigningKey()))
      case "signingPublicKey": result(store.signingPublicKey().map { FlutterStandardTypedData(bytes: $0) })
      case "deleteSigningKey": store.deleteSigningKey(); result(nil)
      case "eraseAll": store.eraseAll(); result(nil)
      case "sign":
        // Off the main thread: the Face ID prompt blocks until the person answers.
        let data = bytes("data"), reason = args["reason"] as? String ?? ""
        DispatchQueue.global(qos: .userInitiated).async {
          let out: [String: Any]
          switch store.sign(data, reason: reason) {
          case .signed(let s): out = ["result": "signed", "signature": FlutterStandardTypedData(bytes: s)]
          case .cancelled: out = ["result": "cancelled"]
          case .invalidated: out = ["result": "invalidated"]
          case .failed(let m): out = ["result": "failed", "message": m]
          }
          DispatchQueue.main.async { result(out) }
        }
      default: result(FlutterMethodNotImplemented)
      }
    } catch let e as KeyStore.KeyError {
      result(FlutterError(code: "key_error", message: e.message, details: nil))
    } catch {
      result(FlutterError(code: "key_error", message: error.localizedDescription, details: nil))
    }
  }
}
