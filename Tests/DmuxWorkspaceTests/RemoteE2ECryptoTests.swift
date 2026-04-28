import CryptoKit
import XCTest
@testable import DmuxWorkspace

final class RemoteE2ECryptoTests: XCTestCase {
    func testMatchCodeUsesStableSharedFormula() {
        XCTAssertEqual(
            RemoteE2ECrypto.matchCode(
                hostPublicKey: "host-public-key",
                devicePublicKey: "device-public-key",
                pairingCode: "205503D6",
                pairingSecret: "pairing-secret"
            ),
            "8EC-D5F"
        )
    }

    func testEncryptedPayloadRoundTripsWithAuthenticatedContext() throws {
        let key = SymmetricKey(size: .bits256)
        let plaintext = Data(#"{"type":"terminal.input","payload":{"data":"q"}}"#.utf8)
        let encrypted = try RemoteE2ECrypto.encrypt(
            plaintext: plaintext,
            key: key,
            hostID: "host-1",
            deviceID: "device-1"
        )

        let decrypted = try RemoteE2ECrypto.decrypt(
            encryptedPayload: encrypted,
            key: key,
            hostID: "host-1",
            deviceID: "device-1"
        )

        XCTAssertEqual(decrypted, plaintext)
        XCTAssertThrowsError(
            try RemoteE2ECrypto.decrypt(
                encryptedPayload: encrypted,
                key: key,
                hostID: "host-1",
                deviceID: "other-device"
            )
        )
    }
}
