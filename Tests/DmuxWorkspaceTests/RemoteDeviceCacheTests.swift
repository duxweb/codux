import XCTest
@testable import DmuxWorkspace

final class RemoteDeviceCacheTests: XCTestCase {
    func testRemoteSettingsDecodeOldPayloadWithoutCachedDevices() throws {
        let data = Data(
            #"""
            {
              "isEnabled": true,
              "serverURL": "http://10.0.0.225:8088",
              "hostID": "host-1",
              "hostToken": "token-1",
              "hostPrivateKey": "private",
              "hostPublicKey": "public"
            }
            """#.utf8
        )

        let settings = try JSONDecoder().decode(AppRemoteSettings.self, from: data)

        XCTAssertTrue(settings.isEnabled)
        XCTAssertEqual(settings.hostID, "host-1")
        XCTAssertEqual(settings.cachedDevices, [])
        XCTAssertEqual(settings.displayCachedDevices, [])
    }

    func testDeviceCacheKeepsOnlyActiveDevicesForCurrentHost() {
        var settings = AppRemoteSettings()
        settings.hostID = "host-1"

        let active = device(id: "device-1", hostID: "host-1", name: "Phone", lastSeen: Date(timeIntervalSince1970: 200), online: true)
        let revoked = device(
            id: "device-2",
            hostID: "host-1",
            name: "Revoked",
            lastSeen: Date(timeIntervalSince1970: 300),
            revokedAt: Date(timeIntervalSince1970: 400),
            online: true
        )
        let otherHost = device(id: "device-3", hostID: "host-2", name: "Other", lastSeen: Date(timeIntervalSince1970: 500), online: true)

        settings.cacheDevices([revoked, otherHost, active])

        XCTAssertEqual(settings.cachedDevices.map(\.id), ["device-1"])
        XCTAssertEqual(settings.displayCachedDevices.first?.online, false)
    }

    func testRemoveCachedDevice() {
        var settings = AppRemoteSettings()
        settings.hostID = "host-1"
        settings.cacheDevices([
            device(id: "device-1", hostID: "host-1", name: "A"),
            device(id: "device-2", hostID: "host-1", name: "B"),
        ])

        settings.removeCachedDevice(id: "device-1")

        XCTAssertEqual(settings.cachedDevices.map(\.id), ["device-2"])
    }

    private func device(
        id: String,
        hostID: String,
        name: String,
        lastSeen: Date = Date(timeIntervalSince1970: 100),
        revokedAt: Date? = nil,
        online: Bool? = nil
    ) -> RemoteHostDevice {
        RemoteHostDevice(
            id: id,
            hostId: hostID,
            name: name,
            publicKey: "public-key",
            createdAt: Date(timeIntervalSince1970: 10),
            lastSeen: lastSeen,
            revokedAt: revokedAt,
            online: online
        )
    }
}
