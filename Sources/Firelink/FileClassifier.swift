import Foundation

enum FileClassifier {
    private static let musicExtensions: Set<String> = [
        "aac", "aif", "aiff", "alac", "amr", "ape", "au", "caf", "flac", "m4a",
        "m4b", "mid", "midi", "mp3", "oga", "ogg", "opus", "ra", "wav", "weba",
        "wma"
    ]

    private static let movieExtensions: Set<String> = [
        "3g2", "3gp", "avi", "divx", "f4v", "flv", "m2ts", "m4v", "mkv", "mov",
        "mp4", "mpeg", "mpg", "mts", "ogm", "ogv", "rm", "rmvb", "ts", "vob",
        "webm", "wmv"
    ]

    private static let compressedExtensions: Set<String> = [
        "7z", "ace", "alz", "apk", "appx", "ar", "arc", "arj", "bz", "bz2",
        "cab", "cpio", "deb", "dmg", "gz", "gzip", "iso", "jar", "lha", "lzh",
        "lz", "lz4", "lzip", "lzma", "pak", "pkg", "rar", "rpm", "sit", "sitx",
        "tar", "tbz", "tbz2", "tgz", "tlz", "txz", "war", "whl", "xar", "xz",
        "z", "zip", "zipx", "zst"
    ]

    private static let pictureExtensions: Set<String> = [
        "ai", "apng", "avif", "bmp", "cr2", "cr3", "dng", "emf", "eps", "gif",
        "heic", "heif", "ico", "indd", "jfif", "jpeg", "jpg", "jxl", "nef",
        "orf", "pbm", "pgm", "png", "pnm", "ppm", "psd", "raw", "rw2",
        "svg", "tga", "tif", "tiff", "webp", "wmf"
    ]

    private static let documentExtensions: Set<String> = [
        "azw", "azw3", "csv", "djvu", "doc", "docm", "docx", "dot", "dotx",
        "epub", "fb2", "htm", "html", "ics", "key", "log", "md", "mobi", "pdf",
        "numbers", "odp", "ods", "odt", "pages", "pot", "potx", "pps", "ppsx",
        "ppt", "pptm", "pptx", "rtf", "tex", "txt", "vcf", "xls", "xlsm",
        "xlsx", "xml", "xps", "yaml", "yml"
    ]

    static func category(forFileName fileName: String) -> DownloadCategory {
        let ext = (fileName as NSString).pathExtension.lowercased()
        guard !ext.isEmpty else { return .other }

        if musicExtensions.contains(ext) { return .musics }
        if movieExtensions.contains(ext) { return .movies }
        if compressedExtensions.contains(ext) { return .compressed }
        if pictureExtensions.contains(ext) { return .pictures }
        if documentExtensions.contains(ext) { return .documents }
        return .other
    }

    static func destinationDirectory(for category: DownloadCategory) -> URL {
        let downloads = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Downloads")
        return downloads.appendingPathComponent(category.rawValue, isDirectory: true)
    }

    static func fileName(from url: URL) -> String {
        let pathName = url.lastPathComponent.removingPercentEncoding ?? url.lastPathComponent
        if !pathName.isEmpty, pathName != "/" {
            return pathName
        }

        let host = url.host(percentEncoded: false) ?? "download"
        return "\(host)-download"
    }
}
