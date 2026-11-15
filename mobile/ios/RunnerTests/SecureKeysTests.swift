import Security
import XCTest

@testable import Runner

/// The key store on the simulator. The Secure Enclave key with biometric access control cannot be made there (error
/// -1020), so signing runs on the plain Keychain key of the debug-only software mode; the Face ID gate itself is a check
/// on a real iPhone.
final class SecureKeysTests: XCTestCase {
  private var service = ""
  private var marker: URL!
  private var stores: [KeyStore] = []

  override func setUpWithError() throws {
    service = "dev.bobix.phone.tests.\(UUID().uuidString)"
    marker = FileManager.default.temporaryDirectory.appendingPathComponent("\(service)-marker")
  }

  override func tearDown() {
    stores.forEach { $0.eraseAll() }
    try? FileManager.default.removeItem(at: marker)
  }

  private func makeStore(software: Bool = true) throws -> KeyStore {
    let s = try KeyStore(service: service, software: software, markerURL: marker)
    stores.append(s)
    return s
  }

  func testSecretsRoundTripOverwriteAndDelete() throws {
    let s = try makeStore()
    XCTAssertNil(s.secret("auth"))
    try s.putSecret("auth", Data([1, 2, 3]))
    try s.putSecret("box", Data([9]))
    XCTAssertEqual(s.secret("auth"), Data([1, 2, 3]))
    try s.putSecret("auth", Data([4]))
    XCTAssertEqual(s.secret("auth"), Data([4]))
    XCTAssertEqual(s.secret("box"), Data([9]))
    s.deleteSecret("auth")
    XCTAssertNil(s.secret("auth"))
    XCTAssertEqual(s.secret("box"), Data([9]))
  }

  func testSecretsStayOnThisDeviceAndOutOfBackups() throws {
    let s = try makeStore()
    try s.putSecret("auth", Data([1]))
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: "auth",
      kSecReturnAttributes as String: true,
    ]
    var out: CFTypeRef?
    XCTAssertEqual(SecItemCopyMatching(query as CFDictionary, &out), errSecSuccess)
    let attributes = try XCTUnwrap(out as? [String: Any])
    XCTAssertEqual(attributes[kSecAttrAccessible as String] as? String, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly as String)
    // Not synchronised with iCloud Keychain (0 or absent both mean "no").
    XCTAssertNotEqual(attributes[kSecAttrSynchronizable as String] as? Int, 1)
    // The marker file, and with it the app's data folder, is left out of backups.
    let values = try marker.resourceValues(forKeys: [.isExcludedFromBackupKey])
    XCTAssertEqual(values.isExcludedFromBackup, true)
  }

  func testSecretsLiveInTheSharedAccessGroup() throws {
    let group = try XCTUnwrap(KeyStore.bundleGroup(), "Info.plist names the group the extensions share")
    XCTAssertTrue(group.hasSuffix("dev.bobix.phone.keys"))
    let s = try makeStore()
    try s.putSecret("auth", Data([1]))
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: "auth",
      kSecReturnAttributes as String: true,
    ]
    var out: CFTypeRef?
    XCTAssertEqual(SecItemCopyMatching(query as CFDictionary, &out), errSecSuccess)
    XCTAssertEqual((out as? [String: Any])?[kSecAttrAccessGroup as String] as? String, group)
  }

  func testEraseAllAlsoClearsAnItemWrittenToTheDefaultGroup() throws {
    let s = try makeStore()
    // What an earlier build did: no access group, so the app's own default one.
    let legacy: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: "auth",
      kSecValueData as String: Data([7]),
      kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
    ]
    XCTAssertEqual(SecItemAdd(legacy as CFDictionary, nil), errSecSuccess)
    try s.putSecret("box", Data([8]))
    try FileManager.default.removeItem(at: marker)
    let again = try makeStore() // a fresh install: erases everything first
    XCTAssertNil(again.secret("box"))
    var out: CFTypeRef?
    let anyGroup: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: "auth"]
    XCTAssertEqual(SecItemCopyMatching(anyGroup as CFDictionary, &out), errSecItemNotFound, "the old item is gone too")
  }

  func testSigningKeyMakesSignaturesThePublicKeyVerifies() throws {
    let s = try makeStore()
    XCTAssertNil(s.signingPublicKey())
    let publicData = try s.createSigningKey()
    XCTAssertEqual(publicData.count, 65, "an uncompressed P-256 point")
    XCTAssertEqual(s.signingPublicKey(), publicData)

    let message = Data("the bytes of a command".utf8)
    guard case .signed(let signature) = s.sign(message, reason: "test") else { return XCTFail("no signature") }
    let publicKey = try XCTUnwrap(SecKeyCreateWithData(publicData as CFData, [
      kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
      kSecAttrKeyClass as String: kSecAttrKeyClassPublic,
    ] as CFDictionary, nil))
    XCTAssertTrue(SecKeyVerifySignature(publicKey, .ecdsaSignatureMessageX962SHA256, message as CFData, signature as CFData, nil))
    XCTAssertFalse(SecKeyVerifySignature(publicKey, .ecdsaSignatureMessageX962SHA256, Data("another".utf8) as CFData, signature as CFData, nil), "a signature fits one message")
  }

  func testANewSigningKeyReplacesTheOldOne() throws {
    let s = try makeStore()
    let first = try s.createSigningKey()
    let second = try s.createSigningKey()
    XCTAssertNotEqual(first, second)
    XCTAssertEqual(s.signingPublicKey(), second)
  }

  func testWithoutAKeySigningSaysPairAgain() throws {
    let s = try makeStore()
    XCTAssertEqual(s.sign(Data([1]), reason: "test"), .invalidated)
    _ = try s.createSigningKey()
    s.deleteSigningKey()
    XCTAssertEqual(s.sign(Data([1]), reason: "test"), .invalidated)
  }

  func testAFreshInstallErasesEveryOldKey() throws {
    let s = try makeStore()
    try s.putSecret("auth", Data([1]))
    _ = try s.createSigningKey()
    // Deleting the app removes the marker (it lives in the app's folder) but the Keychain keeps its items.
    try FileManager.default.removeItem(at: marker)

    let again = try makeStore()
    XCTAssertNil(again.secret("auth"), "an old key never comes back")
    XCTAssertNil(again.signingPublicKey())
    XCTAssertTrue(FileManager.default.fileExists(atPath: marker.path))
  }

  func testAnOrdinaryLaunchKeepsTheKeys() throws {
    let s = try makeStore()
    try s.putSecret("auth", Data([1]))
    let publicData = try s.createSigningKey()
    let again = try makeStore()
    XCTAssertEqual(again.secret("auth"), Data([1]))
    XCTAssertEqual(again.signingPublicKey(), publicData)
  }

  func testEraseAllClearsSecretsAndTheSigningKey() throws {
    let s = try makeStore()
    try s.putSecret("auth", Data([1]))
    try s.putSecret("box", Data([2]))
    _ = try s.createSigningKey()
    s.eraseAll()
    XCTAssertNil(s.secret("auth"))
    XCTAssertNil(s.secret("box"))
    XCTAssertNil(s.signingPublicKey())
  }
}
