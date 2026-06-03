import SwiftUI

enum AppTheme: String, Codable, CaseIterable, Identifiable, Sendable {
    case system = "System Default"
    case light = "Light"
    case dark = "Modern Dark"
    case dracula = "Dracula"
    case nord = "Nord"

    var id: String { rawValue }

    var theme: Theme {
        switch self {
        case .system:
            return Theme.system
        case .light:
            return Theme.modernLight
        case .dark:
            return Theme.modernDark
        case .dracula:
            return Theme.dracula
        case .nord:
            return Theme.nord
        }
    }

    var colorScheme: ColorScheme? {
        switch self {
        case .system: return nil
        case .light: return .light
        case .dark, .dracula, .nord: return .dark
        }
    }
}

struct Theme: Equatable, Sendable {
    var background: Color?
    var secondaryBackground: Color?
    var text: Color?
    var secondaryText: Color?
    var accent: Color?

    static let system = Theme(
        background: nil,
        secondaryBackground: nil,
        text: nil,
        secondaryText: nil,
        accent: nil
    )

    static let modernLight = Theme(
        background: Color(nsColor: NSColor(calibratedRed: 0.98, green: 0.98, blue: 0.99, alpha: 1.0)),
        secondaryBackground: Color(nsColor: NSColor(calibratedRed: 0.94, green: 0.94, blue: 0.96, alpha: 1.0)),
        text: Color.primary,
        secondaryText: Color.secondary,
        accent: Color.accentColor
    )

    static let modernDark = Theme(
        background: Color(nsColor: NSColor(calibratedRed: 0.08, green: 0.08, blue: 0.10, alpha: 1.0)),
        secondaryBackground: Color(nsColor: NSColor(calibratedRed: 0.12, green: 0.12, blue: 0.14, alpha: 1.0)),
        text: Color(nsColor: NSColor(calibratedRed: 0.9, green: 0.9, blue: 0.95, alpha: 1.0)),
        secondaryText: Color(nsColor: NSColor(calibratedRed: 0.6, green: 0.6, blue: 0.65, alpha: 1.0)),
        accent: Color(nsColor: NSColor(calibratedRed: 0.35, green: 0.55, blue: 0.95, alpha: 1.0))
    )

    static let dracula = Theme(
        background: Color(nsColor: NSColor(calibratedRed: 0.16, green: 0.16, blue: 0.21, alpha: 1.0)),
        secondaryBackground: Color(nsColor: NSColor(calibratedRed: 0.27, green: 0.28, blue: 0.35, alpha: 1.0)),
        text: Color(nsColor: NSColor(calibratedRed: 0.97, green: 0.97, blue: 0.95, alpha: 1.0)),
        secondaryText: Color(nsColor: NSColor(calibratedRed: 0.38, green: 0.44, blue: 0.58, alpha: 1.0)),
        accent: Color(nsColor: NSColor(calibratedRed: 1.00, green: 0.47, blue: 0.65, alpha: 1.0)) // Pink
    )

    static let nord = Theme(
        background: Color(nsColor: NSColor(calibratedRed: 0.18, green: 0.20, blue: 0.25, alpha: 1.0)), // nord0
        secondaryBackground: Color(nsColor: NSColor(calibratedRed: 0.23, green: 0.26, blue: 0.32, alpha: 1.0)), // nord1
        text: Color(nsColor: NSColor(calibratedRed: 0.85, green: 0.87, blue: 0.91, alpha: 1.0)), // nord4
        secondaryText: Color(nsColor: NSColor(calibratedRed: 0.57, green: 0.63, blue: 0.70, alpha: 1.0)), // nord3
        accent: Color(nsColor: NSColor(calibratedRed: 0.53, green: 0.75, blue: 0.82, alpha: 1.0)) // nord8
    )
}

struct AppThemeModifier: ViewModifier {
    let theme: AppTheme

    func body(content: Content) -> some View {
        content
            .preferredColorScheme(theme.colorScheme)
            .tint(theme.theme.accent)
            // Apply custom background only if it's not system.
            // Using .background() on the root view might not cover the whole window. 
            // In macOS, it's often better to just set the color scheme and tint, 
            // and let specific views (like Sidebar, Content) use the theme colors explicitly.
    }
}
