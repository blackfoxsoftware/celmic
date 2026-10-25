import Foundation
import Network

/// Browses `_celmic._udp` via Bonjour. The desktop puts its IPv4 candidates and
/// port in the TXT record, so no separate resolve step is needed.
final class Discovery {
  private let browser: NWBrowser
  private let queue = DispatchQueue(label: "dev.celmic.discovery")
  private var found: [String: [String: Any]] = [:]

  init() {
    browser = NWBrowser(for: .bonjourWithTXTRecord(type: "_celmic._udp", domain: "local."), using: .udp)
  }

  func discover(timeout: TimeInterval, completion: @escaping ([[String: Any]]) -> Void) {
    browser.browseResultsChangedHandler = { [weak self] results, _ in
      self?.record(results)
    }
    browser.start(queue: queue)
    queue.asyncAfter(deadline: .now() + timeout) { [self] in
      browser.cancel()
      completion(Array(found.values))
    }
  }

  private func record(_ results: Set<NWBrowser.Result>) {
    for result in results {
      guard case let .bonjour(txt) = result.metadata else { continue }
      var serviceName: String?
      if case let .service(name, _, _, _) = result.endpoint { serviceName = name }

      let hosts = (txt["ips"] ?? "")
        .split(separator: ",")
        .map { $0.trimmingCharacters(in: .whitespaces) }
        .filter { !$0.isEmpty }
      guard !hosts.isEmpty, let port = Int(txt["port"] ?? "") else { continue }

      let name = txt["name"] ?? serviceName ?? "Computer"
      found[name] = ["name": name, "hosts": hosts, "port": port]
    }
  }
}
