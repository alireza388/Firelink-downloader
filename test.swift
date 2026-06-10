import Foundation
print(Bundle.main.infoDictionary ?? "No Info.plist")
let execPath = Bundle.main.executablePath
print("Exec path: \(execPath ?? "nil")")
if let path = execPath, let attr = try? FileManager.default.attributesOfItem(atPath: path), let modDate = attr[.modificationDate] as? Date {
    print("Mod date: \(modDate)")
}
