// ตัวช่วยของ e2e/update-published.sh: อ่านและกดปุ่มในหน้าต่างของแอปผ่าน Accessibility ของ macOS
// ใช้กับ release build ซึ่งไม่มี hook ของเทสต์ — ทำสิ่งเดียวกับที่ผู้ใช้ทำ คือกดปุ่มที่เห็นบนจอ
//   ax trusted | ax <pid> list | ax <pid> roles | ax <pid> click <ชื่อปุ่มตรงตัว>
import ApplicationServices
import Foundation

let args = CommandLine.arguments
if args.count >= 2, args[1] == "trusted" {
  // โปรแกรมที่เรียก (Terminal, VS Code …) ต้องได้สิทธิ์ใน System Settings → Privacy & Security → Accessibility
  exit(AXIsProcessTrusted() ? 0 : 1)
}
guard args.count >= 3, let pid = pid_t(args[1]) else {
  FileHandle.standardError.write("usage: ax trusted | ax <pid> list | roles | click <ชื่อปุ่มตรงตัว>\n".data(using: .utf8)!)
  exit(64)
}
let app = AXUIElementCreateApplication(pid)
AXUIElementSetMessagingTimeout(app, 3)
// WebKit สร้างต้นไม้ accessibility ของหน้าเว็บเมื่อมีผู้ช่วยเหลือการเข้าถึงขอเท่านั้น
AXUIElementSetAttributeValue(app, "AXManualAccessibility" as CFString, kCFBooleanTrue)
AXUIElementSetAttributeValue(app, "AXEnhancedUserInterface" as CFString, kCFBooleanTrue)
if args[2] == "list" {
  var w: AnyObject?
  let r = AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &w)
  FileHandle.standardError.write("trusted=\(AXIsProcessTrusted()) windows=\((w as? [AXUIElement])?.count ?? -1) err=\(r.rawValue)\n".data(using: .utf8)!)
}

func attr(_ e: AXUIElement, _ name: String) -> AnyObject? {
  var v: AnyObject?
  return AXUIElementCopyAttributeValue(e, name as CFString, &v) == .success ? v : nil
}
func label(_ e: AXUIElement) -> String {
  for k in [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute] {
    if let s = attr(e, k as String) as? String, !s.isEmpty { return s }
  }
  return ""
}
var found: [(String, String, AXUIElement)] = []
func walk(_ e: AXUIElement, _ depth: Int) {
  if depth > 60 || found.count > 400 { return }
  let role = attr(e, kAXRoleAttribute as String) as? String ?? ""
  if role == "AXButton" || role == "AXStaticText" || role == "AXHeading" || role == "AXLink" {
    var text = label(e)
    if text.isEmpty, let kids = attr(e, kAXChildrenAttribute as String) as? [AXUIElement] {
      text = kids.map(label).joined(separator: " ")
    }
    found.append((role, text, e))
  }
  if let kids = attr(e, kAXChildrenAttribute as String) as? [AXUIElement] {
    for k in kids { walk(k, depth + 1) }
  }
}
walk(app, 0)
if args[2] == "roles" {
  var seen: [String: Int] = [:]
  func count(_ e: AXUIElement, _ d: Int) {
    if d > 60 { return }
    seen[attr(e, kAXRoleAttribute as String) as? String ?? "?", default: 0] += 1
    for k in (attr(e, kAXChildrenAttribute as String) as? [AXUIElement] ?? []) { count(k, d + 1) }
  }
  count(app, 0)
  print(seen)
}
if args[2] == "list" {
  for (role, text, _) in found where !text.isEmpty { print("\(role)\t\(text)") }
} else if args[2] == "click" {
  let needle = args[3]
  let clean = { (t: String) in t.trimmingCharacters(in: .whitespacesAndNewlines) }
  if let hit = found.first(where: { $0.0 == "AXButton" && clean($0.1) == needle }) {
    let r = AXUIElementPerformAction(hit.2, kAXPressAction as CFString)
    print(r == .success ? "clicked: \(hit.1)" : "press failed: \(r.rawValue)")
    exit(r == .success ? 0 : 2)
  }
  print("not found: \(needle)")
  exit(1)
}
