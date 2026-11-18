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
                .task {
                    while !Task.isCancelled {
                        try? await Task.sleep(for: .seconds(15))
                        guard !Task.isCancelled else { return }
                        if scenePhase == .active { await store.refresh() }
                    }
                }
                .onReceive(NotificationCenter.default.publisher(for: UIApplication.willEnterForegroundNotification)) { _ in
                    Task { await store.refresh() }
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
    static let navy = adaptive(dark: 0.035, 0.075, 0.12, light: 0.965, 0.973, 0.969)
    static let panel = adaptive(dark: 0.075, 0.13, 0.18, light: 1, 1, 1)
    // Dark-mode teal stays vivid; light-mode teal is darker so links and labels remain readable.
    static let teal = adaptive(dark: 0.25, 0.82, 0.72, light: 0.02, 0.42, 0.37)
    static let cream = adaptive(dark: 0.94, 0.92, 0.84, light: 0.10, 0.14, 0.16)
    static let muted = adaptive(dark: 0.66, 0.74, 0.75, light: 0.31, 0.37, 0.39)
    static let codeBackground = adaptive(dark: 0.015, 0.035, 0.05, light: 0.91, 0.93, 0.93)
    static let warning = adaptive(dark: 1, 0.68, 0.30, light: 0.52, 0.28, 0.04)

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
                        .tabItem { Label("Requests", systemImage: "checkmark.shield") }.tag(0)
                    NavigationStack { GrantsView().toolbar { actions } }
                        .tabItem { Label("Apps", systemImage: "square.grid.2x2") }.tag(1)
                    NavigationStack { VaultView().toolbar { actions } }
                        .tabItem { Label("Keys", systemImage: "key.horizontal") }.tag(2)
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
            if store.account != nil || store.demoMode {
                Button { activeSheet = .connect } label: { Image(systemName: "qrcode.viewfinder") }.accessibilityLabel("Connect an app")
            }
            Button { activeSheet = .settings } label: { Image(systemName: "gearshape") }.accessibilityLabel("Settings")
        }
    }
}
