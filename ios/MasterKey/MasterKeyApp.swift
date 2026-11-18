import SwiftUI
import UIKit

@main
struct MasterKeyApp: App {
    @Environment(\.scenePhase) private var scenePhase
    @UIApplicationDelegateAdaptor(APNSDelegate.self) private var appDelegate
    @StateObject private var store = AppStore()
    @AppStorage("appearance") private var appearance = AppearancePreference.system.rawValue

    private var preferredScheme: ColorScheme? {
        switch AppearancePreference(rawValue: appearance) ?? .system {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(store)
                .preferredColorScheme(preferredScheme)
                .tint(MKStyle.teal)
                .task { await store.start() }
                .task(id: scenePhase) {
                    guard scenePhase == .active else { return }
                    while !Task.isCancelled {
                        await store.refresh()
                        do { try await Task.sleep(for: .seconds(5)) }
                        catch { return }
                    }
                }
                .onReceive(NotificationCenter.default.publisher(for: .masterKeyAPNSToken)) { note in
                    if let token = note.object as? String { Task { await store.uploadDeviceToken(token) } }
                }
                .onReceive(NotificationCenter.default.publisher(for: .masterKeyRequestTapped)) { note in
                    if let id = note.object as? String {
                        store.pendingRequestID = id
                        Task { await store.refresh() }
                    }
                }
                .onReceive(NotificationCenter.default.publisher(for: .masterKeyAPNSError)) { note in
                    if let message = note.object as? String { store.infoMessage = "Push registration unavailable: \(message). Foreground refresh still works." }
                }
        }
    }
}

enum MKStyle {
    static let navy = adaptive(dark: 0.09, 0.085, 0.075, light: 250 / 255, 247 / 255, 242 / 255)
    static let panel = adaptive(dark: 0.14, 0.135, 0.12, light: 1, 1, 1)
    static let teal = adaptive(dark: 0.55, 0.76, 0.62, light: 61 / 255, 122 / 255, 87 / 255)
    static let cream = adaptive(dark: 250 / 255, 247 / 255, 242 / 255, light: 31 / 255, 27 / 255, 22 / 255)
    static let muted = adaptive(dark: 0.70, 0.67, 0.62, light: 122 / 255, 114 / 255, 104 / 255)
    static let line = adaptive(dark: 0.24, 0.225, 0.20, light: 236 / 255, 230 / 255, 220 / 255)
    static let codeBackground = adaptive(dark: 0.07, 0.065, 0.055, light: 239 / 255, 234 / 255, 225 / 255)
    static let warning = adaptive(dark: 1, 0.72, 0.40, light: 0.57, 0.30, 0.09)
    static let buttonText = adaptive(dark: 0.08, 0.11, 0.09, light: 1, 1, 1)

    private static func adaptive(dark r: CGFloat, _ g: CGFloat, _ b: CGFloat, light lr: CGFloat, _ lg: CGFloat, _ lb: CGFloat) -> Color {
        Color(uiColor: UIColor { traits in
            traits.userInterfaceStyle == .dark
                ? UIColor(red: r, green: g, blue: b, alpha: 1)
                : UIColor(red: lr, green: lg, blue: lb, alpha: 1)
        })
    }
}

enum AppearancePreference: String, CaseIterable, Identifiable {
    case system, light, dark
    var id: String { rawValue }
    var title: String { rawValue.capitalized }
}

struct AppearancePicker: View {
    @AppStorage("appearance") private var appearance = AppearancePreference.system.rawValue

    var body: some View {
        Picker("Appearance", selection: $appearance) {
            ForEach(AppearancePreference.allCases) { option in
                Text(option.title).tag(option.rawValue)
            }
        }
        .pickerStyle(.segmented)
        .accessibilityLabel("Appearance")
    }
}

struct RootView: View {
    @EnvironmentObject private var store: AppStore
    @State private var selection = 0
    @State private var activeSheet: RootSheet?
    private enum RootSheet: Identifiable {
        case settings, connect, computer
        case enroll(String)
        var id: String {
            switch self {
            case .settings: "settings"
            case .connect: "connect"
            case .computer: "computer"
            case .enroll(let id): "enroll:" + id
            }
        }
    }

    var body: some View {
        Group {
            if store.account == nil && !store.demoMode {
                NavigationStack { WelcomeView().toolbar { actions } }
            } else {
                TabView(selection: $selection) {
                    NavigationStack { InboxView().toolbar { actions } }
                        .tabItem { Label("Inbox", systemImage: "tray") }.tag(0)
                    NavigationStack { GrantsView().toolbar { actions } }
                        .tabItem { Label("Apps", systemImage: "square.grid.2x2") }.tag(1)
                    NavigationStack { VaultView().toolbar { actions } }
                        .tabItem { Label("Keys", systemImage: "key.horizontal") }.tag(2)
                    NavigationStack { SettingsView() }
                        .tabItem { Label("Settings", systemImage: "gearshape") }.tag(3)
                }
            }
        }
        .background(MKStyle.navy)
        .toolbarBackground(MKStyle.navy, for: .tabBar)
        .toolbarBackground(.visible, for: .tabBar)
        .alert("MasterKey", isPresented: $store.showError) {
            Button("OK", role: .cancel) { }
        } message: { Text(store.errorMessage) }
        .sheet(item: $activeSheet) { sheet in
            NavigationStack {
                switch sheet {
                case .settings:
                    SettingsView().toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { activeSheet = nil } } }
                case .connect: ConnectAppView()
                case .computer: ComputerKeySetupView()
                case .enroll(let id): EnrollmentView(id: id).id(id)
                }
            }
        }
        .onOpenURL { url in
            if store.handleOpenURL(url) { presentPendingLink() }
        }
        .onContinueUserActivity(NSUserActivityTypeBrowsingWeb) { activity in
            if let url = activity.webpageURL, store.handleOpenURL(url) { presentPendingLink() }
        }
        .onChange(of: store.linkGeneration) { _, _ in presentPendingLink() }
        .onReceive(NotificationCenter.default.publisher(for: .masterKeyAddFromComputer)) { note in
            store.beginComputerKeySetup(returnTo: note.object as? String)
            activeSheet = .computer
        }
        .onReceive(NotificationCenter.default.publisher(for: .masterKeyOpenKeys)) { _ in selection = 2 }
        .onReceive(NotificationCenter.default.publisher(for: .masterKeyConnectApp)) { _ in activeSheet = .connect }
        .onChange(of: store.pendingRequestID) { _, id in if id != nil { selection = 0 } }
    }

    private func presentPendingLink() {
        if let id = store.pendingEnrollmentID { activeSheet = .enroll(id) }
        else if store.pendingConnectCode != nil { activeSheet = .connect }
    }

    @ToolbarContentBuilder private var actions: some ToolbarContent {
        ToolbarItemGroup(placement: .topBarTrailing) {
            if (store.account != nil || store.demoMode) && (selection == 1 || selection == 2) {
                Button { activeSheet = .connect } label: { Image(systemName: "qrcode.viewfinder") }.accessibilityLabel("Connect an app")
            }
            if store.account == nil && !store.demoMode {
                Button { activeSheet = .settings } label: { Image(systemName: "gearshape") }.accessibilityLabel("Settings")
            }
        }
    }
}
