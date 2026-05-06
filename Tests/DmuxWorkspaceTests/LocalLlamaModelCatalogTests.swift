import XCTest
@testable import DmuxWorkspace

final class LocalLlamaModelCatalogTests: XCTestCase {
    func testBundledManifestContainsInstallableModelsFromLowToHigh() {
        let manifest = LocalLlamaModelCatalog.bundledManifest()

        XCTAssertEqual(manifest.schemaVersion, 1)
        XCTAssertGreaterThanOrEqual(manifest.models.count, 29)
        XCTAssertEqual(manifest.models.first?.id, "qwen2.5-0.5b-instruct-q4-k-m")
        XCTAssertEqual(manifest.models.last?.id, "llama3.3-70b-instruct-q4-k-m")
        XCTAssertNotNil(LocalLlamaModelCatalog.descriptor(id: LocalLlamaModelCatalog.defaultModelID, in: manifest.models))
    }

    func testBundledManifestPrioritizesModelScopeDownloads() {
        let manifest = LocalLlamaModelCatalog.bundledManifest()

        for descriptor in manifest.models {
            XCTAssertFalse(descriptor.downloadSources.isEmpty, descriptor.id)
            XCTAssertEqual(descriptor.preferredDownloadSources.first?.id, "modelscope", descriptor.id)
            XCTAssertTrue(
                descriptor.preferredDownloadSources.first?.url.absoluteString
                    .contains("www.modelscope.cn") == true,
                descriptor.id
            )
            XCTAssertFalse(descriptor.sha256.isEmpty, descriptor.id)
            XCTAssertGreaterThan(descriptor.byteCount, 0, descriptor.id)
        }
    }

    func testDownloadRouteCanPreferInternationalSources() {
        let manifest = LocalLlamaModelCatalog.bundledManifest()

        for descriptor in manifest.models {
            XCTAssertEqual(
                descriptor.preferredDownloadSources(for: .china).first?.id,
                "modelscope",
                descriptor.id
            )
            XCTAssertEqual(
                descriptor.preferredDownloadSources(for: .international).first?.id,
                "huggingface",
                descriptor.id
            )
        }
    }

    func testBundledManifestDescriptionsCoverSupportedAppLanguages() {
        let manifest = LocalLlamaModelCatalog.bundledManifest()
        let requiredLanguageKeys = Set([
            "en", "zh-Hans", "zh-Hant", "ja", "ko", "fr", "de", "es", "pt-BR", "ru",
        ])

        for descriptor in manifest.models {
            XCTAssertTrue(
                requiredLanguageKeys.isSubset(of: Set(descriptor.localizedDescription.keys)),
                descriptor.id
            )
            for key in requiredLanguageKeys {
                XCTAssertFalse(descriptor.localizedDescription[key, default: ""].isEmpty, "\(descriptor.id) \(key)")
            }
        }

        for descriptor in manifest.serverModels {
            XCTAssertTrue(
                requiredLanguageKeys.isSubset(of: Set(descriptor.localizedDescription.keys)),
                descriptor.id
            )
            for key in requiredLanguageKeys {
                XCTAssertFalse(descriptor.localizedDescription[key, default: ""].isEmpty, "\(descriptor.id) \(key)")
            }
        }
    }

    func testBundledManifestIncludesInternationalModelFamilies() {
        let descriptors = Dictionary(
            uniqueKeysWithValues: LocalLlamaModelCatalog.bundledManifest().models.map {
                ($0.id, $0)
            }
        )

        XCTAssertEqual(descriptors["gemma3-1b-it-q4-0"]?.chatTemplate, "gemma")
        XCTAssertEqual(descriptors["gemma3-4b-it-q4-0"]?.chatTemplate, "gemma")
        XCTAssertTrue(descriptors["gemma3-1b-it-q4-0"]?.tasks.contains("international") == true)
        XCTAssertTrue(descriptors["gemma3-4b-it-q4-0"]?.tasks.contains("international") == true)
        XCTAssertEqual(descriptors["phi3-mini-4k-instruct-q4"]?.chatTemplate, "phi3")
        XCTAssertEqual(descriptors["phi4-q4-0"]?.chatTemplate, "phi4")
        XCTAssertEqual(descriptors["llama3.1-8b-instruct-q4-k-m"]?.chatTemplate, "llama3")
        XCTAssertEqual(descriptors["mistral-small-24b-instruct-2501-q4-k-m"]?.chatTemplate, "mistral-small")
    }

    func testBundledManifestCoversMacMemoryTiersUpTo128GB() {
        let models = LocalLlamaModelCatalog.bundledManifest().models
        let memoryTiers = Set(models.map(\.minimumMemoryGB))

        XCTAssertTrue(memoryTiers.contains(8))
        XCTAssertTrue(memoryTiers.contains(16))
        XCTAssertTrue(memoryTiers.contains(24))
        XCTAssertTrue(memoryTiers.contains(32))
        XCTAssertTrue(memoryTiers.contains(64))
        XCTAssertTrue(memoryTiers.contains(128))
        XCTAssertTrue(models.contains { $0.tasks.contains("code-review") })
        XCTAssertTrue(models.contains { $0.tasks.contains("memory") })
        XCTAssertTrue(models.contains { $0.tasks.contains("international") })
        XCTAssertTrue(models.contains { $0.chatTemplate == "deepseek-r1" })
    }

    func testBundledManifestTracksServerOnlyLatestModels() {
        let serverIDs = Set(LocalLlamaModelCatalog.bundledManifest().serverModels.map(\.id))

        XCTAssertTrue(serverIDs.contains("qwen3.6-35b-a3b"))
        XCTAssertTrue(serverIDs.contains("deepseek-v4-flash"))
        XCTAssertTrue(serverIDs.contains("glm-5"))
        XCTAssertTrue(serverIDs.contains("kimi-k2.6"))
    }
}
