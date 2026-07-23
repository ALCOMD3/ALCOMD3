"use client";

import { QueryClientProvider } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import type React from "react";
import {
	Suspense,
	useCallback,
	useEffect,
	useEffectEvent,
	useLayoutEffect,
} from "react";
import { useTranslation } from "react-i18next";
import { ToastContainer } from "react-toastify";
import { CopyProjectProgressHost } from "@/app/_main/projects/manage/-copy-project";
import Loading from "@/app/-loading";
import { BackupProjectProgressHost } from "@/components/BackupProjectDialog";
import {
	CheckForUpdateMessage,
	handleAutomaticUpdate,
	refreshUpdateCheck,
	shouldSkipUpdateDialogFromReminder,
	UpdateInstallProgressHost,
} from "@/components/CheckForUpdateMessage";
import { ProjectProgressTaskTray } from "@/components/ProjectProgressTaskTray";
import { RestoreBackupProgressHost } from "@/components/RestoreProjectFromBackupDialog";
import { Button } from "@/components/ui/button";
import {
	DialogClose,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { LogEntry, TauriImportTemplateResult } from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { type DialogContext, DialogRoot, openSingleDialog } from "@/lib/dialog";
import { isFindKey, useDocumentEvent } from "@/lib/events";
import { tc } from "@/lib/i18n";
import { processResult } from "@/lib/import-templates";
import {
	applyPersistedMaterialTheme,
	applyStoredMaterialTheme,
	getPersistedMaterialTheme,
} from "@/lib/material-theme";
import { queryClient } from "@/lib/query-client";
import {
	toastError,
	toastSuccess,
	toastThrownError,
	toastWarning,
} from "@/lib/toast";
import { useTauriListen } from "@/lib/use-tauri-listen";

function MaterialThemeInitializer() {
	useLayoutEffect(() => {
		applyStoredMaterialTheme();
		commands.utilFrontendReady().catch((error) => {
			console.error("failed to show main window", error);
		});
	}, []);

	useEffect(() => {
		void applyPersistedMaterialTheme();
	}, []);

	useEffect(() => {
		const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
		const refreshTheme = async () => {
			const settings = await getPersistedMaterialTheme();
			if (settings.mode === "auto") {
				await applyPersistedMaterialTheme();
			}
		};

		const listener = () => void refreshTheme();
		mediaQuery.addEventListener("change", listener);
		return () => mediaQuery.removeEventListener("change", listener);
	}, []);

	return null;
}

function StartupBadHostNameDialog({
	dialog: _dialog,
}: {
	dialog: DialogContext<void>;
}) {
	return (
		<>
			<DialogHeader>
				<DialogTitle className="text-warning text-center">
					{tc("sidebar:dialog:bad hostname")}
				</DialogTitle>
			</DialogHeader>
			<div className="whitespace-normal">
				{tc("sidebar:dialog:bad hostname description")}
			</div>
			<DialogFooter>
				<DialogClose asChild>
					<Button>{tc("general:button:close")}</Button>
				</DialogClose>
			</DialogFooter>
		</>
	);
}

export function Providers({ children }: { children: React.ReactNode }) {
	const navigate = useNavigate();

	const showToastForLog = useEffectEvent((entry: LogEntry) => {
		if (entry.level === "Error" && (entry.gui_toast ?? true)) {
			toastError(entry.message);
		} else if (entry.level === "Warn" && (entry.gui_toast ?? false)) {
			toastWarning(entry.message);
		}
	});

	useTauriListen<LogEntry>("log", (event) => {
		showToastForLog(event.payload);
	});

	useEffect(() => {
		commands.utilGetLogEntries().then((value) => {
			for (const entry of value) {
				showToastForLog(entry);
			}
		});
	}, []);

	const moveToRepositories = useCallback(() => {
		if (location.pathname !== "/packages/repositories") {
			navigate({ to: "/packages/repositories" });
		}
	}, [navigate]);

	useTauriListen<null>("deep-link-add-repository", (_) => {
		moveToRepositories();
	});

	useEffect(() => {
		let cancel = false;
		commands.deepLinkHasAddRepository().then((has) => {
			if (cancel) return;
			if (has) {
				moveToRepositories();
			}
		});
		return () => {
			cancel = true;
		};
	}, [moveToRepositories]);

	useTauriListen<TauriImportTemplateResult>(
		"templates-imported",
		async ({ payload: result }) => {
			try {
				await processResult(result);
			} catch (e) {
				console.error(e);
				toastThrownError(e);
			}
		},
	);

	useEffect(() => {
		(async () => {
			const count = await commands.deepLinkImportedClearNonToastedCount();
			if (count !== 0) {
				toastSuccess(tc("templates:toast:imported n templates", { count }));
			}
		})();
	}, []);

	useEffect(() => {
		let cancel = false;
		(async () => {
			try {
				const isBadHostName = await commands.utilIsBadHostname();
				queryClient.setQueryData(["util_is_bad_hostname"], isBadHostName);
				if (cancel || !isBadHostName) return;
				await openSingleDialog(
					StartupBadHostNameDialog,
					{},
					"large-dialog-content",
				);
			} catch (e) {
				console.error(e);
			}
		})();
		return () => {
			cancel = true;
		};
	}, []);

	const { i18n } = useTranslation();

	useEffect(() => {
		let cancel = false;
		(async () => {
			try {
				if (import.meta.env.DEV) return;
				const checkVersion = await refreshUpdateCheck(false);
				if (cancel) return;
				if (checkVersion?.automatic_update_handled) {
					handleAutomaticUpdate(checkVersion);
					return;
				}
				if (
					checkVersion &&
					!(await shouldSkipUpdateDialogFromReminder(checkVersion))
				) {
					await openSingleDialog(CheckForUpdateMessage, {
						response: checkVersion,
					});
				}
			} catch {
				// refreshUpdateCheck already logs failures; startup checks stay silent.
			}
		})();
		return () => {
			cancel = true;
		};
	}, []);

	useDocumentEvent(
		"keydown",
		(e) => {
			if (isFindKey(e)) {
				e.preventDefault();
			}
		},
		[],
	);

	return (
		<>
			<MaterialThemeInitializer />
			<ToastContainer
				position="bottom-right"
				autoClose={3000}
				hideProgressBar={false}
				newestOnTop={false}
				closeOnClick
				rtl={false}
				pauseOnFocusLoss
				draggable
				pauseOnHover
				theme="light"
				className={"whitespace-normal"}
			/>
			<QueryClientProvider client={queryClient}>
				<TooltipProvider>
					<div lang={i18n.language} className="contents">
						<Suspense fallback={<Loading />}>{children}</Suspense>
					</div>
					<BackupProjectProgressHost />
					<CopyProjectProgressHost />
					<RestoreBackupProgressHost />
					<ProjectProgressTaskTray />
					<UpdateInstallProgressHost />
					<DialogRoot />
				</TooltipProvider>
			</QueryClientProvider>
		</>
	);
}
