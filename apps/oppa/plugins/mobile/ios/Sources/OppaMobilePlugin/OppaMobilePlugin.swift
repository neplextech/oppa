import Foundation
import Network
import Tauri
import UIKit
import WebKit

struct RegisterEventsArgs: Decodable {
    let handler: Channel
}

struct KeepAwakeArgs: Decodable {
    let enabled: Bool
}

struct TaskArgs: Decodable {
    let taskId: Int
}

struct BrowseArgs: Decodable {
    let serviceTypes: [String]
    let timeoutMs: Int
}

struct UrlArgs: Decodable {
    let url: String
}

struct ShareArgs: Decodable {
    let title: String
    let text: String
}

struct MobileEvent: Encodable {
    let type: String
}

struct Endpoint: Encodable {
    let name: String
    let serviceType: String
    let host: String
    let port: Int
}

struct Endpoints: Encodable {
    let services: [Endpoint]
}

struct TaskResult: Encodable {
    let taskId: Int
}

struct LocalNetworkResult: Encodable {
    let access: String
}

/// Native bridge for the OPPA agent on iOS.
///
/// iOS suspends apps shortly after they leave the foreground, so OPPA keeps
/// its connection only while active. This plugin reports lifecycle and
/// connectivity changes, requests bounded background time for in-flight jobs,
/// optionally keeps the screen awake, and performs Bonjour discovery. It never
/// uses audio, location, or other unrelated background modes.
class OppaMobilePlugin: Plugin {
    private var events: Channel?
    private var observers: [NSObjectProtocol] = []
    private let pathMonitor = NWPathMonitor()
    private var lastPathSatisfied: Bool?
    private var backgroundTasks: [Int: UIBackgroundTaskIdentifier] = [:]
    private var nextTaskId = 1
    private let queue = DispatchQueue(label: "com.neplextech.oppa.mobile")

    @objc public override func load(webview: WKWebView) {
        let center = NotificationCenter.default
        observers.append(center.addObserver(
            forName: UIApplication.didEnterBackgroundNotification, object: nil, queue: .main
        ) { [weak self] _ in self?.send("backgrounded") })
        observers.append(center.addObserver(
            forName: UIApplication.willEnterForegroundNotification, object: nil, queue: .main
        ) { [weak self] _ in self?.send("foregrounded") })

        pathMonitor.pathUpdateHandler = { [weak self] path in
            guard let self = self else { return }
            let satisfied = path.status == .satisfied
            if self.lastPathSatisfied == satisfied { return }
            let initial = self.lastPathSatisfied == nil
            self.lastPathSatisfied = satisfied
            if initial { return }
            self.send(satisfied ? "networkAvailable" : "networkLost")
        }
        pathMonitor.start(queue: queue)
    }

    deinit {
        observers.forEach { NotificationCenter.default.removeObserver($0) }
        pathMonitor.cancel()
    }

    private func send(_ type: String) {
        guard let events = events else { return }
        try? events.send(MobileEvent(type: type))
    }

    @objc public func registerEvents(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(RegisterEventsArgs.self)
        events = args.handler
        invoke.resolve()
    }

    @objc public func setKeepAwake(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(KeepAwakeArgs.self)
        DispatchQueue.main.async {
            UIApplication.shared.isIdleTimerDisabled = args.enabled
            invoke.resolve()
        }
    }

    @objc public func beginBackgroundTask(_ invoke: Invoke) {
        DispatchQueue.main.async {
            let taskId = self.nextTaskId
            self.nextTaskId += 1
            let identifier = UIApplication.shared.beginBackgroundTask(withName: "OPPA print job") {
                [weak self] in
                guard let self = self else { return }
                self.send("backgroundTaskExpired")
                self.finishTask(taskId)
            }
            if identifier == .invalid {
                invoke.reject("background execution time is unavailable")
                return
            }
            self.backgroundTasks[taskId] = identifier
            invoke.resolve(TaskResult(taskId: taskId))
        }
    }

    @objc public func endBackgroundTask(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(TaskArgs.self)
        DispatchQueue.main.async {
            self.finishTask(args.taskId)
            invoke.resolve()
        }
    }

    private func finishTask(_ taskId: Int) {
        if let identifier = backgroundTasks.removeValue(forKey: taskId) {
            UIApplication.shared.endBackgroundTask(identifier)
        }
    }

    @objc public func browseServices(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(BrowseArgs.self)
        let types = args.serviceTypes.filter { OppaMobilePlugin.isServiceType($0) }.prefix(8)
        let window = max(500, min(args.timeoutMs, 30_000))
        ServiceBrowser(types: Array(types), timeoutMs: window, queue: queue).start { endpoints in
            invoke.resolve(Endpoints(services: endpoints))
        }
    }

    @objc public func localNetworkAccess(_ invoke: Invoke) {
        LocalNetworkProbe(queue: queue).start { access in
            invoke.resolve(LocalNetworkResult(access: access))
        }
    }

    @objc public func openUrl(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(UrlArgs.self)
        guard let url = URL(string: args.url), url.scheme == "https" || url.scheme == "http" else {
            invoke.reject("only HTTP(S) URLs can be opened")
            return
        }
        DispatchQueue.main.async {
            UIApplication.shared.open(url) { opened in
                opened ? invoke.resolve() : invoke.reject("the URL could not be opened")
            }
        }
    }

    @objc public func shareText(_ invoke: Invoke) throws {
        let args = try invoke.parseArgs(ShareArgs.self)
        DispatchQueue.main.async {
            guard let presenter = OppaMobilePlugin.topViewController() else {
                invoke.reject("sharing is unavailable")
                return
            }
            let sheet = UIActivityViewController(activityItems: [args.text], applicationActivities: nil)
            sheet.setValue(args.title, forKey: "subject")
            // iPad presents the sheet as a popover anchored to the view.
            sheet.popoverPresentationController?.sourceView = presenter.view
            sheet.popoverPresentationController?.sourceRect = CGRect(
                x: presenter.view.bounds.midX, y: presenter.view.bounds.maxY - 1, width: 1, height: 1
            )
            presenter.present(sheet, animated: true)
            invoke.resolve()
        }
    }

    @objc public func openAppSettings(_ invoke: Invoke) {
        DispatchQueue.main.async {
            if let url = URL(string: UIApplication.openSettingsURLString) {
                UIApplication.shared.open(url)
            }
            invoke.resolve()
        }
    }

    /// The front-most view controller of the active window scene.
    static func topViewController() -> UIViewController? {
        let window = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap { $0.windows }
            .first { $0.isKeyWindow }
        var controller = window?.rootViewController
        while let presented = controller?.presentedViewController {
            controller = presented
        }
        return controller
    }

    static func isServiceType(_ value: String) -> Bool {
        value.range(of: "^_[a-z0-9-]{1,32}\\._(tcp|udp)$", options: .regularExpression) != nil
    }
}

/// Browses Bonjour types and resolves each result to a numeric host and port.
///
/// Resolution opens and immediately cancels a TCP connection to the service,
/// the documented way to turn an `NWEndpoint.service` into an address.
private final class ServiceBrowser {
    private let types: [String]
    private let timeoutMs: Int
    private let queue: DispatchQueue
    private var browsers: [NWBrowser] = []
    private var connections: [NWConnection] = []
    private var found: [String: Endpoint] = [:]
    private var finished = false

    init(types: [String], timeoutMs: Int, queue: DispatchQueue) {
        self.types = types
        self.timeoutMs = timeoutMs
        self.queue = queue
    }

    func start(done: @escaping ([Endpoint]) -> Void) {
        for type in types {
            let browser = NWBrowser(for: .bonjour(type: type, domain: nil), using: .tcp)
            browser.browseResultsChangedHandler = { [weak self] results, _ in
                for result in results { self?.resolve(result.endpoint, type: type) }
            }
            browser.start(queue: queue)
            browsers.append(browser)
        }
        queue.asyncAfter(deadline: .now() + .milliseconds(timeoutMs)) {
            self.finished = true
            self.browsers.forEach { $0.cancel() }
            self.connections.forEach { $0.cancel() }
            done(Array(self.found.values))
        }
    }

    private func resolve(_ endpoint: NWEndpoint, type: String) {
        guard !finished, case let .service(name, _, _, _) = endpoint else { return }
        let connection = NWConnection(to: endpoint, using: .tcp)
        connection.stateUpdateHandler = { [weak self, weak connection] state in
            guard let self = self, let connection = connection else { return }
            switch state {
            case .ready:
                if case let .hostPort(host, port) = connection.currentPath?.remoteEndpoint {
                    let address = ServiceBrowser.describe(host)
                    self.found["\(address):\(port.rawValue)"] = Endpoint(
                        name: name, serviceType: type, host: address, port: Int(port.rawValue)
                    )
                }
                connection.cancel()
            case .failed, .waiting:
                connection.cancel()
            default:
                break
            }
        }
        connections.append(connection)
        connection.start(queue: queue)
    }

    private static func describe(_ host: NWEndpoint.Host) -> String {
        switch host {
        case let .ipv4(address):
            return "\(address)".components(separatedBy: "%").first ?? "\(address)"
        case let .ipv6(address):
            return "\(address)".components(separatedBy: "%").first ?? "\(address)"
        case let .name(name, _):
            return name
        @unknown default:
            return "\(host)"
        }
    }
}

/// Starts a short Bonjour browse to trigger and observe the local network
/// privacy decision. iOS reports a denial as a `PolicyDenied` DNS-SD error.
private final class LocalNetworkProbe {
    private let queue: DispatchQueue
    private var browser: NWBrowser?
    private var completed = false

    init(queue: DispatchQueue) {
        self.queue = queue
    }

    func start(done: @escaping (String) -> Void) {
        let browser = NWBrowser(for: .bonjour(type: "_pdl-datastream._tcp", domain: nil), using: .tcp)
        self.browser = browser
        let finish: (String) -> Void = { [weak self] access in
            guard let self = self, !self.completed else { return }
            self.completed = true
            self.browser?.cancel()
            done(access)
        }
        browser.stateUpdateHandler = { state in
            switch state {
            case .ready:
                finish("granted")
            case let .waiting(error), let .failed(error):
                if case let .dns(code) = error, code == -65570 {
                    finish("denied")
                }
            default:
                break
            }
        }
        browser.start(queue: queue)
        queue.asyncAfter(deadline: .now() + 10) { finish("undetermined") }
    }
}

@_cdecl("init_plugin_oppa_mobile")
func initPlugin() -> Plugin {
    return OppaMobilePlugin()
}
