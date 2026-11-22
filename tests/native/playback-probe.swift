import AppKit
import WebKit
let app = NSApplication.shared
// Visibility assertions require a foreground test window.
app.setActivationPolicy(.regular)
class Receiver: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
    private weak var readyWebView: WKWebView?
    private var started = false
    private var observers: [NSObjectProtocol] = []
    override init() {
        super.init()
        for name in [NSApplication.didBecomeActiveNotification, NSWindow.didBecomeKeyNotification] {
            observers.append(NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                self?.startWhenActive()
            })
        }
    }
    deinit {
        for observer in observers { NotificationCenter.default.removeObserver(observer) }
    }
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        readyWebView = webView
        webView.window?.makeKeyAndOrderFront(nil)
        NSApplication.shared.activate(ignoringOtherApps: true)
        startWhenActive()
    }
    private func startWhenActive() {
        guard !started, NSApplication.shared.isActive,
              let webView = readyWebView, webView.window?.isKeyWindow == true else { return }
        started = true
        // Visibility may update after native activation; both must precede audible playback.
        webView.evaluateJavaScript("""
        if (document.visibilityState === 'visible') { run(); }
        else {
            document.addEventListener('visibilitychange', function start() {
                if (document.visibilityState === 'visible') {
                    document.removeEventListener('visibilitychange', start);
                    run();
                }
            });
        }
        void 0;
        """)
    }
    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        if let progress = message.body as? [String: Any], progress["progress"] != nil {
            FileHandle.standardError.write(Data("Progress: \(progress)\n".utf8))
            if progress["next"] as? Bool == true {
                DispatchQueue.main.async { message.webView?.evaluateJavaScript("run(); void 0") }
            }
            return
        }
        if let data = try? JSONSerialization.data(withJSONObject: message.body, options: [.sortedKeys]), let text = String(data: data, encoding: .utf8) { print(text) }
        NSApplication.shared.terminate(nil)
    }
}
let config = WKWebViewConfiguration()
config.mediaTypesRequiringUserActionForPlayback = []
let receiver = Receiver()
config.userContentController.add(receiver, name: "result")
let web = WKWebView(frame: NSRect(x:0,y:0,width:640,height:360), configuration: config)
let window = NSWindow(contentRect: web.frame, styleMask: [.titled], backing: .buffered, defer: false)
web.navigationDelegate = receiver
window.contentView = web
window.level = .floating
window.makeKeyAndOrderFront(nil)
app.activate(ignoringOtherApps: true)
let url = URL(string: CommandLine.arguments[1])!
if url.isFileURL { web.loadFileURL(url, allowingReadAccessTo: URL(fileURLWithPath: "/")) }
else { web.load(URLRequest(url: url)) }
DispatchQueue.main.asyncAfter(deadline: .now()+50) { print("TIMEOUT"); NSApplication.shared.terminate(nil) }
// Command-line Swift programs may release otherwise-unused UI owners before the run loop.
// The visibility gate must observe a live window throughout the complete playback journey.
withExtendedLifetime((window, web, receiver)) {
    app.run()
}
