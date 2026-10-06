import XCTest

/// Runs the commands in the DRIVE environment variable against the app DRIVE_APP (a bundle id,
/// default net.drawnui.hellorust), one per line:
///   launch | activate | home
///   rotate portrait | left | right
///   tap X Y            points
///   long X Y SECONDS
///   swipe X1 Y1 X2 Y2 SECONDS   (a press, a move at that pace, a release)
///   type TEXT
///   wait SECONDS
///   shot NAME          a screenshot attached to the result
///   tree NAME          the app's accessibility tree as text (what VoiceOver gets)
///   press LABEL        taps the accessibility element (any type) with that label
///   audit NAME         Xcode's accessibility audit of the screen, its issues as text
final class Drive: XCTestCase {
    func testDrive() throws {
        let env = ProcessInfo.processInfo.environment
        let app = XCUIApplication(bundleIdentifier: env["DRIVE_APP"] ?? "net.drawnui.hellorust")
        let script = ProcessInfo.processInfo.environment["DRIVE"] ?? "activate\nshot screen"
        func at(_ x: Double, _ y: Double) -> XCUICoordinate {
            app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: y))
        }
        for line in script.split(separator: "\n") {
            let p = line.split(separator: " ", maxSplits: 1).map(String.init)
            let rest = p.count > 1 ? p[1] : ""
            let n = rest.split(separator: " ").compactMap { Double($0) }
            switch p[0] {
            case "launch": app.launch()
            case "activate": app.activate()
            case "home": XCUIDevice.shared.press(.home)
            case "rotate":
                XCUIDevice.shared.orientation = ["left": .landscapeLeft, "right": .landscapeRight][rest] ?? .portrait
            case "tap": at(n[0], n[1]).tap()
            case "long": at(n[0], n[1]).press(forDuration: n[2])
            case "swipe":
                at(n[0], n[1]).press(forDuration: 0.05, thenDragTo: at(n[2], n[3]),
                                     withVelocity: XCUIGestureVelocity(CGFloat(hypot(n[2] - n[0], n[3] - n[1]) / max(n[4], 0.01))),
                                     thenHoldForDuration: 0)
            case "type": app.typeText(rest)
            case "wait": Thread.sleep(forTimeInterval: n[0])
            case "shot":
                let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
                a.name = rest
                a.lifetime = .keepAlways
                add(a)
            case "press":
                app.descendants(matching: .any)[rest].firstMatch.tap()
            case "audit":
                var issues = ""
                try app.performAccessibilityAudit { issue in
                    issues += "\(issue.auditType): \(issue.compactDescription) | \(issue.detailedDescription) | element: \(issue.element?.debugDescription.split(separator: "\n").first ?? "-")\n"
                    return true
                }
                let a = XCTAttachment(string: issues.isEmpty ? "no issues" : issues)
                a.name = rest + ".txt"
                a.lifetime = .keepAlways
                add(a)
            case "tree":
                let a = XCTAttachment(string: app.debugDescription)
                a.name = rest + ".txt"
                a.lifetime = .keepAlways
                add(a)
            default: XCTFail("unknown command \(line)")
            }
        }
    }
}
