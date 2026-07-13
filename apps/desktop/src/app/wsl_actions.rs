use super::*;

impl CoduxApp {
    pub(in crate::app) fn load_wsl_distribution_catalog(&mut self, cx: &mut Context<Self>) {
        if !cfg!(target_os = "windows")
            || !self.state.settings.wsl_enabled
            || self.wsl_distribution_catalog_loading
            || self.wsl_install_progress.is_some()
        {
            return;
        }
        self.wsl_distribution_catalog_loading = true;
        self.wsl_distribution_catalog = None;
        self.wsl_runtime_error = None;
        self.invalidate_ui_region(cx, UiRegion::Root);
        let runtime_service = self.runtime_service.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                runtime_service.wsl_distribution_catalog()
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to inspect WSL distributions: {error}")));
            let _ = this.update(cx, |app, cx| {
                app.wsl_distribution_catalog_loading = false;
                match result {
                    Ok(catalog) => {
                        app.select_available_wsl_distribution(&catalog);
                        app.wsl_distribution_catalog = Some(catalog);
                    }
                    Err(error) => app.wsl_runtime_error = Some(error),
                }
                app.invalidate_ui_region(cx, UiRegion::Root);
            });
        })
        .detach();
    }

    pub(in crate::app) fn set_wsl_selected_distribution(
        &mut self,
        distribution: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .wsl_distribution_catalog
            .as_ref()
            .is_some_and(|catalog| {
                catalog.distributions.iter().any(|status| {
                    !status.distribution_installed && status.distribution == distribution
                })
            })
        {
            self.wsl_selected_distribution = distribution;
            self.invalidate_ui_region(cx, UiRegion::Root);
        }
    }

    pub(in crate::app) fn install_wsl_distribution(
        &mut self,
        distribution: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_wsl_install(distribution, true, cx);
    }

    pub(in crate::app) fn install_wsl_runtime(
        &mut self,
        distribution: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_wsl_install(distribution, false, cx);
    }

    fn start_wsl_install(
        &mut self,
        distribution: String,
        install_distribution: bool,
        cx: &mut Context<Self>,
    ) {
        if self.wsl_install_progress.is_some() || distribution.trim().is_empty() {
            return;
        }
        let operation = if install_distribution {
            codux_runtime::wsl::WslInstallOperation::Distribution
        } else {
            codux_runtime::wsl::WslInstallOperation::Runtime
        };
        self.wsl_install_progress = Some(codux_runtime::wsl::WslInstallProgress {
            distribution: distribution.clone(),
            operation,
            percent: None,
            message: String::new(),
        });
        self.wsl_runtime_error = None;
        self.invalidate_ui_region(cx, UiRegion::Root);
        let runtime_service = self.runtime_service.clone();
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let (progress_tx, progress_rx) =
                flume::unbounded::<codux_runtime::wsl::WslInstallProgress>();
            let install_distribution_name = distribution.clone();
            let install_task = codux_runtime::async_runtime::spawn_blocking(move || {
                let forward = move |progress| {
                    let _ = progress_tx.send(progress);
                };
                if install_distribution {
                    runtime_service
                        .install_wsl_distribution_with_progress(&install_distribution_name, forward)
                } else {
                    runtime_service
                        .install_wsl_runtime_with_progress(&install_distribution_name, forward)
                }
            });
            loop {
                while let Ok(progress) = progress_rx.try_recv() {
                    let _ = this.update(cx, |app, cx| {
                        app.wsl_install_progress = Some(progress);
                        app.invalidate_ui_region(cx, UiRegion::Root);
                    });
                }
                if install_task.is_finished() {
                    break;
                }
                timer.timer(std::time::Duration::from_millis(80)).await;
            }
            while let Ok(progress) = progress_rx.try_recv() {
                let _ = this.update(cx, |app, cx| {
                    app.wsl_install_progress = Some(progress);
                    app.invalidate_ui_region(cx, UiRegion::Root);
                });
            }
            let result = install_task.await;
            let _ = this.update(cx, |app, cx| {
                app.wsl_install_progress = None;
                match result {
                    Ok(Ok(())) => app.load_wsl_distribution_catalog(cx),
                    Ok(Err(error)) => app.wsl_runtime_error = Some(error),
                    Err(error) => app.wsl_runtime_error = Some(error.to_string()),
                }
                app.invalidate_ui_region(cx, UiRegion::Root);
            });
        })
        .detach();
    }

    fn select_available_wsl_distribution(
        &mut self,
        catalog: &codux_runtime::wsl::WslDistributionCatalog,
    ) {
        let selection_is_available = catalog.distributions.iter().any(|status| {
            !status.distribution_installed && status.distribution == self.wsl_selected_distribution
        });
        if !selection_is_available {
            self.wsl_selected_distribution = catalog
                .distributions
                .iter()
                .find(|status| !status.distribution_installed)
                .map(|status| status.distribution.clone())
                .unwrap_or_default();
        }
    }
}
