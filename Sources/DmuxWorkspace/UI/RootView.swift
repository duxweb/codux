import AppKit
import SwiftUI

struct RootView: View {
    let model: AppModel

    private let titlebarHeight: CGFloat = 42
    private let collapsedSidebarWidth: CGFloat = 70

    var body: some View {
        ZStack(alignment: .top) {
            AppWindowGlassBackground(tintColor: model.windowGlassTintColor)

            HStack(spacing: 0) {
                SidebarView(model: model)
                    .frame(
                        minWidth: model.isSidebarExpanded ? 248 : collapsedSidebarWidth,
                        idealWidth: model.isSidebarExpanded ? 248 : collapsedSidebarWidth,
                        maxWidth: model.isSidebarExpanded ? 248 : collapsedSidebarWidth
                    )
                    .fixedSize(horizontal: true, vertical: false)

                TerminalHorizontalSplitContainer(model: model)
                    .frame(minWidth: 700, maxWidth: .infinity, maxHeight: .infinity)
            }
            .padding(.top, titlebarHeight)

            TitlebarOverlayView(model: model)
                .frame(height: titlebarHeight)
        }
        .background(Color.clear)
        .background(MainWorkspaceWindowConfigurator())
        .ignoresSafeArea(.container, edges: .top)
        .onAppear {
            model.noteRootViewAppeared()
            PetDesktopWindowPresenter.sync(model: model)
        }
        .onChange(of: model.appSettings.pet.enabled) { _, _ in
            PetDesktopWindowPresenter.sync(model: model)
        }
        .onChange(of: model.appSettings.pet.desktopWidgetEnabled) { _, _ in
            PetDesktopWindowPresenter.sync(model: model)
        }
        .onChange(of: model.appSettings.pet.staticMode) { _, _ in
            PetDesktopWindowPresenter.sync(model: model)
        }
        .onChange(of: model.petStore.isClaimed) { _, _ in
            PetDesktopWindowPresenter.sync(model: model)
        }
        .onChange(of: model.petStore.currentExperienceTokens) { _, _ in
            PetDesktopWindowPresenter.sync(model: model)
        }
        .onChange(of: model.petStore.currentHatchTokens) { _, _ in
            PetDesktopWindowPresenter.sync(model: model)
        }
    }
}
