// Bonjour discovery of boompi boxes on the local network. boompid
// advertises `_boompi._tcp` via avahi (rust/boompid/src/netname.rs):
// the DNS-SD instance name is the speaker name and the TXT record
// carries the connection contract (`id` = stable box id, `proto`,
// `ver`, `path`). This is the Wi-Fi counterpart of the BLE scan -
// same boxes, same protocol, different pipe.

import Foundation
import Network

/// One box seen on the LAN. `key` is the TXT `id` when present
/// (stable across renames), the instance name otherwise (old boxes
/// whose baseline advert predates boompid's first write).
public struct NetworkBox: Equatable {
    public let key: String
    public let name: String
    public let endpoint: NWEndpoint
}

/// Long-running `_boompi._tcp` browser. Results are pushed to
/// `onChange` on the main actor; the browser restarts itself after
/// transient failures (Wi-Fi flaps, mDNSResponder restarts).
@MainActor
final class NetworkDiscovery {
    static let serviceType = "_boompi._tcp"

    var onChange: (([NetworkBox]) -> Void)?
    private(set) var boxes: [NetworkBox] = []
    private var browser: NWBrowser?

    func start() {
        guard browser == nil else { return }
        let browser = NWBrowser(
            for: .bonjourWithTXTRecord(type: Self.serviceType, domain: nil),
            using: NWParameters()
        )
        self.browser = browser
        browser.stateUpdateHandler = { [weak self] state in
            // Started on the main queue: assumeIsolated is sound.
            MainActor.assumeIsolated {
                guard let self, self.browser === browser else { return }
                if case .failed = state {
                    browser.cancel()
                    self.browser = nil
                    Task { [weak self] in
                        try? await Task.sleep(nanoseconds: 2_000_000_000)
                        self?.start()
                    }
                }
            }
        }
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            MainActor.assumeIsolated {
                guard let self, self.browser === browser else { return }
                self.apply(results)
            }
        }
        browser.start(queue: .main)
    }

    func stop() {
        browser?.cancel()
        browser = nil
        boxes = []
    }

    private func apply(_ results: Set<NWBrowser.Result>) {
        var out: [NetworkBox] = []
        for result in results {
            guard case .service(let name, _, _, _) = result.endpoint else { continue }
            var key = name
            if case .bonjour(let txt) = result.metadata,
               let id = txt.dictionary["id"], !id.isEmpty {
                key = id
            }
            // Multi-interface boxes show up once per path; keep one.
            if !out.contains(where: { $0.key == key }) {
                out.append(NetworkBox(key: key, name: name, endpoint: result.endpoint))
            }
        }
        out.sort { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
        if out != boxes {
            boxes = out
            onChange?(out)
        }
    }
}
