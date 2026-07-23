import type { DefaultError } from "@tanstack/query-core";
import { queryOptions, type UseMutationOptions } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { CheckCircle2, CircleAlert, Minimize2, RefreshCw } from "lucide-react";
import type React from "react";
import { Fragment, useSyncExternalStore } from "react";
import { DelayedButton } from "@/components/DelayedButton";
import { ExternalLink } from "@/components/ExternalLink";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { ScrollArea } from "@/components/ui/scroll-area";
import { assertNever } from "@/lib/assert-never";
import type {
	TauriBasePackageInfo,
	TauriPackage,
	TauriPackageChange,
	TauriPendingProjectChanges,
	TauriProjectApplyProgress,
	TauriVersion,
} from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { type DialogContext, openSingleDialog } from "@/lib/dialog";
import { isHandleable } from "@/lib/errors";
import { tc, tt } from "@/lib/i18n";
import { queryClient } from "@/lib/query-client";
import { toastInfo, toastSuccess, toastThrownError } from "@/lib/toast";
import { groupBy, keyComparator } from "@/lib/utils";
import { compareVersion, toVersionString } from "@/lib/version";

export type RequestedOperation =
	| {
			type: "install";
			pkg: TauriPackage;
			hasUnityIncompatibleLatest?: boolean;
	  }
	| {
			type: "upgradeAll";
			hasUnityIncompatibleLatest: boolean;
			packages: TauriPackage[];
	  }
	| {
			type: "resolve";
	  }
	| {
			type: "reinstallAll";
	  }
	| {
			type: "remove";
			displayName: string;
			packageId: string;
	  }
	| {
			type: "bulkInstalled";
			hasUnityIncompatibleLatest: boolean;
			packages: TauriPackage[];
	  }
	| {
			type: "bulkReinstalled";
			packageIds: string[];
	  }
	| {
			type: "bulkRemoved";
			packageIds: string[];
	  };

function environmentPackages(projectPath: string) {
	return queryOptions({
		queryKey: ["projectDetails", projectPath],
		queryFn: () => commands.projectDetails(projectPath),
		refetchOnWindowFocus: false,
	});
}

function mutationOptions<
	TOptions extends UseMutationOptions<TData, TError, TVariables, TContext>,
	TData = unknown,
	TError = DefaultError,
	TVariables = void,
	TContext = unknown,
>(
	options: TOptions & UseMutationOptions<TData, TError, TVariables, TContext>,
): TOptions {
	return options;
}

export function applyChangesMutation(projectPath: string) {
	return mutationOptions({
		mutationKey: ["projectApplyChanges", projectPath],
		mutationFn: async (operation: RequestedOperation) =>
			await applyChanges(projectPath, operation),
		onError: (e) => {
			console.error(e);
			toastThrownError(e);
		},
		onSettled: async () => {
			document.dispatchEvent(new Event("post-package-changes"));
			await queryClient.invalidateQueries({
				queryKey: ["projectDetails", projectPath],
			});
			await queryClient.invalidateQueries({
				queryKey: ["environmentPackages"],
			});
			await queryClient.invalidateQueries({
				queryKey: ["projectPackageRows", projectPath],
			});
		},
	});
}

let packageChangeRequestInProgress = false;

export async function applyChanges(
	projectPath: string,
	operation: RequestedOperation,
) {
	if (packageChangeRequestInProgress) return;
	packageChangeRequestInProgress = true;
	try {
		const existingPackages = queryClient.getQueryData(
			environmentPackages(projectPath).queryKey,
		)?.installed_packages;

		const changes = await createChanges(projectPath, operation);
		if (
			!(await openSingleDialog(ProjectChangesDialog, {
				changes,
				existingPackages,
			}))
		) {
			// close window
			return;
		}
		startPackageApplyProgress(
			projectPath,
			operation,
			changes,
			existingPackages,
		);
		await applyPendingChangesWithProgress(projectPath, changes.changes_version);
		finishPackageApplyProgress();
		showToast(operation);
	} catch (e) {
		failPackageApplyProgress();
		if (isHandleable(e) && e.body.type === "MissingDependencies") {
			await openSingleDialog(MissingDependenciesDialog, {
				dependencies: e.body.dependencies,
			});
		} else {
			throw e;
		}
	} finally {
		packageChangeRequestInProgress = false;
	}
}

function createChanges(
	projectPath: string,
	operation: RequestedOperation,
): Promise<TauriPendingProjectChanges> {
	switch (operation.type) {
		case "install":
			return commands.projectInstallPackages(projectPath, [
				[operation.pkg.name, toVersionString(operation.pkg.version)],
			]);
		case "upgradeAll":
			return commands.projectInstallPackages(
				projectPath,
				operation.packages.map((pkg) => [
					pkg.name,
					toVersionString(pkg.version),
				]),
			);
		case "resolve":
		case "reinstallAll":
			return commands.projectResolve(projectPath);
		case "remove":
			return commands.projectRemovePackages(projectPath, [operation.packageId]);
		case "bulkInstalled":
			return commands.projectInstallPackages(
				projectPath,
				operation.packages.map((pkg) => [
					pkg.name,
					toVersionString(pkg.version),
				]),
			);
		case "bulkReinstalled":
			return commands.projectReinstallPackages(
				projectPath,
				operation.packageIds,
			);
		case "bulkRemoved":
			return commands.projectRemovePackages(projectPath, operation.packageIds);
		default:
			assertNever(operation);
	}
}

async function applyPendingChangesWithProgress(
	projectPath: string,
	changesVersion: number,
) {
	const channel = `project_apply:${Date.now()}_${Math.random().toString(36).substring(7)}`;
	const unlisten = await listen<TauriProjectApplyProgress>(channel, (event) =>
		updatePackageApplyProgress(event.payload),
	);
	try {
		await commands.projectApplyPendingChanges(
			channel,
			projectPath,
			changesVersion,
		);
	} finally {
		unlisten();
	}
}

function showToast(requested: RequestedOperation) {
	switch (requested.type) {
		case "install":
			toastSuccess(
				tt("projects:manage:toast:package installed", {
					name: requested.pkg.display_name ?? requested.pkg.name,
					version: toVersionString(requested.pkg.version),
				}),
			);
			if (requested.hasUnityIncompatibleLatest) {
				toastInfo(
					tt(
						"projects:manage:toast:the package has newer latest with incompatible unity",
					),
				);
			}
			break;
		case "remove":
			toastSuccess(
				tt("projects:manage:toast:package removed", {
					name: requested.displayName,
				}),
			);
			break;
		case "resolve":
			toastSuccess(tt("projects:manage:toast:resolved"));
			break;
		case "reinstallAll":
			toastSuccess(tt("projects:manage:toast:all packages reinstalled"));
			break;
		case "upgradeAll":
			toastSuccess(tt("projects:manage:toast:all packages upgraded"));
			if (requested.hasUnityIncompatibleLatest) {
				toastInfo(
					tt(
						"projects:manage:toast:some package has newer latest with incompatible unity",
					),
				);
			}
			break;
		case "bulkInstalled":
			toastSuccess(tt("projects:manage:toast:selected packages installed"));
			if (requested.hasUnityIncompatibleLatest) {
				toastInfo(
					tt(
						"projects:manage:toast:some package has newer latest with incompatible unity",
					),
				);
			}
			break;
		case "bulkRemoved":
			toastSuccess(tt("projects:manage:toast:selected packages removed"));
			break;
		case "bulkReinstalled":
			toastSuccess(tt("projects:manage:toast:selected packages reinstalled"));
			break;
		default:
			assertNever(requested);
	}
}

type PackageApplyProgressStatus =
	| "applying"
	| "finalizing"
	| "completed"
	| "failed";
type PackageApplyProgressKind = "install" | "remove" | "reinstall";

type PackageApplyProgressItem = {
	packageId: string;
	displayName: string;
	status:
		| "waiting"
		| "downloading"
		| "extracting"
		| "extracted"
		| "installing"
		| "removing"
		| "completed"
		| "failed";
	downloadFinished: boolean;
	extractFinished: boolean;
	applyFinished: boolean;
};

type PackageApplyProgressState = {
	status: PackageApplyProgressStatus;
	minimized: boolean;
	cancelRequested: boolean;
	projectPath: string;
	kind: PackageApplyProgressKind;
	completedSteps: number;
	totalSteps: number;
	items: PackageApplyProgressItem[];
	existingPackages?: [string, TauriBasePackageInfo][];
	retryInstalls: [string, string][];
	retryRemoves: string[];
};

let packageApplyProgressState: PackageApplyProgressState | null = null;
const packageApplyProgressListeners = new Set<() => void>();

function emitPackageApplyProgress() {
	for (const listener of packageApplyProgressListeners) listener();
}

function setPackageApplyProgressState(state: PackageApplyProgressState | null) {
	packageApplyProgressState = state;
	emitPackageApplyProgress();
}

function subscribePackageApplyProgress(listener: () => void) {
	packageApplyProgressListeners.add(listener);
	return () => packageApplyProgressListeners.delete(listener);
}

function getPackageApplyProgressSnapshot() {
	return packageApplyProgressState;
}

function startPackageApplyProgress(
	projectPath: string,
	operation: RequestedOperation,
	changes: TauriPendingProjectChanges,
	existingPackages?: [string, TauriBasePackageInfo][],
) {
	startPackageApplyProgressWithKind(
		projectPath,
		packageApplyProgressKindFromOperation(operation),
		changes,
		existingPackages,
	);
}

function startPackageApplyProgressWithKind(
	projectPath: string,
	kind: PackageApplyProgressKind,
	changes: TauriPendingProjectChanges,
	existingPackages?: [string, TauriBasePackageInfo][],
) {
	const existingPackageMap = new Map(existingPackages ?? []);
	const categorizedChanges = changes.package_changes
		.filter(([_, change]) => isPackageApplyProgressTarget(kind, change))
		.map(([pkgId, change]) =>
			categorizeChange(pkgId, change, existingPackageMap),
		);
	const items = categorizedChanges.map((categorized) => {
		return {
			packageId: categorized.packageId,
			displayName: categorized.displayName,
			status: "waiting" as const,
			downloadFinished: false,
			extractFinished: false,
			applyFinished: false,
		};
	});
	setPackageApplyProgressState({
		status: "applying",
		minimized: false,
		cancelRequested: false,
		projectPath,
		kind,
		completedSteps: 0,
		totalSteps: items.length * packageApplyProgressStepCount(kind),
		items,
		existingPackages,
		retryInstalls: changes.package_changes
			.map(([id, change]) =>
				change.InstallNew !== undefined
					? ([id, toVersionString(change.InstallNew.version)] as [
							string,
							string,
						])
					: undefined,
			)
			.filter((change) => change != null),
		retryRemoves: changes.package_changes
			.map(([id, change]) => ("Remove" in change ? id : undefined))
			.filter((id) => id != null),
	});
}

function updatePackageApplyProgress(progress: TauriProjectApplyProgress) {
	if (packageApplyProgressState == null) return;
	const currentState = packageApplyProgressState;
	const items = currentState.items.map((item) => {
		if (item.packageId !== progress.package_name) return item;
		switch (progress.type) {
			case "DownloadStarted":
				if (currentState.kind === "remove") return item;
				return { ...item, status: "downloading" as const };
			case "DownloadFinished":
				if (currentState.kind === "remove") return item;
				return {
					...item,
					status: "extracting" as const,
					downloadFinished: true,
				};
			case "ExtractStarted":
				if (currentState.kind === "remove") return item;
				return { ...item, status: "extracting" as const };
			case "ExtractFinished":
				if (currentState.kind === "remove") return item;
				return {
					...item,
					status: "extracted" as const,
					extractFinished: true,
				};
			case "InstallStarted":
				if (currentState.kind === "remove") return item;
				return { ...item, status: "installing" as const };
			case "InstallFinished":
				if (currentState.kind === "remove") return item;
				return {
					...item,
					status: "completed" as const,
					downloadFinished: true,
					extractFinished: true,
					applyFinished: true,
				};
			case "RemoveStarted":
				if (currentState.kind !== "remove") return item;
				return { ...item, status: "removing" as const };
			case "RemoveFinished":
				if (currentState.kind !== "remove") return item;
				return {
					...item,
					status: "completed" as const,
					downloadFinished: false,
					extractFinished: false,
					applyFinished: true,
				};
			case "Failed":
				return { ...item, status: "failed" as const };
			default:
				return assertNever(progress);
		}
	});
	const completedSteps = items.reduce(
		(total, item) =>
			total +
			(item.downloadFinished ? 1 : 0) +
			(item.extractFinished ? 1 : 0) +
			(item.applyFinished ? 1 : 0),
		0,
	);
	const status =
		currentState.status === "applying" &&
		items.length > 0 &&
		items.every((item) => item.status === "completed")
			? "finalizing"
			: currentState.status;
	setPackageApplyProgressState({
		...currentState,
		status,
		completedSteps,
		items,
	});
}

function finishPackageApplyProgress() {
	if (packageApplyProgressState == null) return;
	const state = packageApplyProgressState;
	setPackageApplyProgressState({
		...state,
		status: "completed",
		minimized: false,
		completedSteps: state.totalSteps,
		items: state.items.map((item) => ({
			...item,
			status: "completed",
			downloadFinished: state.kind !== "remove",
			extractFinished: state.kind !== "remove",
			applyFinished: true,
		})),
	});
}

function failPackageApplyProgress() {
	if (packageApplyProgressState == null) return;
	const hasFailedPackage = packageApplyProgressState.items.some(
		(item) => item.status === "failed",
	);
	const preserveCompleted =
		packageApplyProgressState.cancelRequested || hasFailedPackage;
	setPackageApplyProgressState({
		...packageApplyProgressState,
		status: "failed",
		minimized: false,
		items: packageApplyProgressState.items.map((item) => {
			if (item.status === "failed") return item;
			if (preserveCompleted && item.status === "completed") return item;
			return {
				...item,
				status: "failed",
			};
		}),
	});
}

function minimizePackageApplyProgress() {
	if (packageApplyProgressState == null) return;
	setPackageApplyProgressState({
		...packageApplyProgressState,
		minimized: true,
	});
}

function restorePackageApplyProgress() {
	if (packageApplyProgressState == null) return;
	setPackageApplyProgressState({
		...packageApplyProgressState,
		minimized: false,
	});
}

function closePackageApplyProgress() {
	setPackageApplyProgressState(null);
}

async function cancelPackageApplyProgress() {
	if (packageApplyProgressState == null) return;
	setPackageApplyProgressState({
		...packageApplyProgressState,
		cancelRequested: true,
	});
	try {
		await commands.projectCancelApplyPendingChanges();
	} catch (e) {
		console.error(e);
		toastThrownError(e);
	}
}

async function retryPackageApplyProgress() {
	const state = packageApplyProgressState;
	if (state == null || state.status !== "failed") return;

	const failedIds = new Set(
		state.items
			.filter((item) => item.status === "failed")
			.map((item) => item.packageId),
	);
	if (failedIds.size === 0) return;

	try {
		const changes = await createRetryChanges(state, failedIds);
		startPackageApplyProgressWithKind(
			state.projectPath,
			state.kind,
			changes,
			state.existingPackages,
		);
		await applyPendingChangesWithProgress(
			state.projectPath,
			changes.changes_version,
		);
		finishPackageApplyProgress();
		document.dispatchEvent(new Event("post-package-changes"));
		await queryClient.invalidateQueries({
			queryKey: ["projectDetails", state.projectPath],
		});
		await queryClient.invalidateQueries({
			queryKey: ["environmentPackages"],
		});
		await queryClient.invalidateQueries({
			queryKey: ["projectPackageRows", state.projectPath],
		});
	} catch (e) {
		failPackageApplyProgress();
		if (isHandleable(e) && e.body.type === "MissingDependencies") {
			await openSingleDialog(MissingDependenciesDialog, {
				dependencies: e.body.dependencies,
			});
		} else {
			console.error(e);
			toastThrownError(e);
		}
	}
}

function createRetryChanges(
	state: PackageApplyProgressState,
	failedIds: Set<string>,
) {
	switch (state.kind) {
		case "install": {
			const installs = state.retryInstalls.filter(([id]) => failedIds.has(id));
			return commands.projectInstallPackages(state.projectPath, installs);
		}
		case "remove": {
			const removes = state.retryRemoves.filter((id) => failedIds.has(id));
			return commands.projectRemovePackages(state.projectPath, removes);
		}
		case "reinstall":
			return commands.projectReinstallPackages(state.projectPath, [
				...failedIds,
			]);
		default:
			return assertNever(state.kind);
	}
}

export function PackageApplyProgressHost() {
	const state = useSyncExternalStore(
		subscribePackageApplyProgress,
		getPackageApplyProgressSnapshot,
	);

	if (state == null) return null;

	const title =
		state.status === "completed"
			? completedPackageApplyProgressTitle(state.kind)
			: state.status === "failed"
				? tc("projects:manage:progress:failed")
				: state.status === "finalizing"
					? tc("projects:manage:progress:finalizing")
					: tc("projects:manage:progress:title");
	const completedCount = state.items.filter(
		(item) => item.status === "completed",
	).length;
	const failedCount = state.items.filter(
		(item) => item.status === "failed",
	).length;
	const showCloseButton =
		state.status === "completed" || state.status === "failed";
	const canCancel = canCancelPackageApplyProgress(state);

	return (
		<>
			<Dialog
				open={!state.minimized}
				onOpenChange={(open) => {
					if (!open && !showCloseButton) {
						minimizePackageApplyProgress();
					}
				}}
			>
				<DialogContent
					className="max-w-xl"
					onEscapeKeyDown={(event) => {
						if (!showCloseButton) {
							event.preventDefault();
							minimizePackageApplyProgress();
						} else {
							event.preventDefault();
							closePackageApplyProgress();
						}
					}}
				>
					<DialogHeader>
						<DialogTitle className="flex items-center gap-2">
							{state.status === "completed" ? (
								<CheckCircle2 className="size-5 text-success" />
							) : state.status === "failed" ? (
								<CircleAlert className="size-5 text-destructive" />
							) : (
								<RefreshCw className="size-5 animate-spin" />
							)}
							{title}
						</DialogTitle>
						<DialogDescription>
							{tc("projects:manage:progress:description", {
								projectPath: state.projectPath,
							})}
						</DialogDescription>
					</DialogHeader>
					<div className="space-y-2">
						<Progress value={state.completedSteps} max={state.totalSteps} />
						<p className="text-center text-sm text-muted-foreground">
							{tc("projects:manage:progress:percent", {
								percent:
									state.totalSteps === 0
										? 100
										: Math.round(
												(state.completedSteps / state.totalSteps) * 100,
											),
							})}
						</p>
						<p className="text-center text-sm text-muted-foreground">
							{tc("projects:manage:progress:summary", {
								completed: completedCount,
								failed: failedCount,
							})}
						</p>
					</div>
					<div className="overflow-hidden rounded-[1rem] bg-secondary/40">
						<ScrollArea className="h-[40vh]">
							<div className="p-2">
								{state.items.map((item) => (
									<div
										className="flex items-center justify-between gap-3 p-2"
										key={item.packageId}
									>
										<div className="min-w-0">
											<p className="font-normal">{item.displayName}</p>
											<p className="text-sm opacity-60">{item.packageId}</p>
										</div>
										<p
											className={`shrink-0 text-sm ${packageApplyProgressStatusClass(item.status)}`}
										>
											{packageApplyProgressStatusLabel(item.status)}
										</p>
									</div>
								))}
							</div>
						</ScrollArea>
					</div>
					<DialogFooter>
						{showCloseButton ? (
							<>
								{state.status === "failed" && (
									<Button className="gap-2" onClick={retryPackageApplyProgress}>
										<RefreshCw className="size-4" />
										{tc("projects:manage:progress:retry")}
									</Button>
								)}
								<Button onClick={closePackageApplyProgress}>
									{tc("general:button:close")}
								</Button>
							</>
						) : (
							<>
								<Button
									className="gap-2"
									disabled={!canCancel || state.cancelRequested}
									onClick={cancelPackageApplyProgress}
								>
									{state.cancelRequested
										? tc("projects:manage:progress:cancelling")
										: tc("projects:manage:progress:cancel")}
								</Button>
								<Button
									className="gap-2"
									onClick={minimizePackageApplyProgress}
								>
									<Minimize2 className="size-4" />
									{tc("projects:manage:progress:minimize")}
								</Button>
							</>
						)}
					</DialogFooter>
				</DialogContent>
			</Dialog>
			{state.minimized && (
				<Button
					className="fixed bottom-4 right-4 z-50 gap-2 shadow-2xl"
					onClick={restorePackageApplyProgress}
				>
					<RefreshCw className="size-4 animate-spin" />
					{tc("projects:manage:progress:restore")}
				</Button>
			)}
		</>
	);
}

function canCancelPackageApplyProgress(state: PackageApplyProgressState) {
	if (state.status !== "applying") return false;
	return state.items.every(
		(item) =>
			item.status !== "installing" &&
			item.status !== "removing" &&
			item.status !== "completed",
	);
}

function packageApplyProgressStatusLabel(
	status: PackageApplyProgressItem["status"],
) {
	switch (status) {
		case "waiting":
			return tc("projects:manage:progress:status:waiting");
		case "downloading":
			return tc("projects:manage:progress:status:downloading");
		case "extracting":
			return tc("projects:manage:progress:status:extracting");
		case "extracted":
			return tc("projects:manage:progress:status:extracted");
		case "installing":
			return tc("projects:manage:progress:status:installing");
		case "removing":
			return tc("projects:manage:progress:status:removing");
		case "completed":
			return tc("projects:manage:progress:status:completed");
		case "failed":
			return tc("projects:manage:progress:status:failed");
		default:
			assertNever(status);
	}
}

function packageApplyProgressStatusClass(
	status: PackageApplyProgressItem["status"],
) {
	switch (status) {
		case "completed":
			return "text-success";
		case "failed":
			return "text-destructive";
		case "waiting":
		case "downloading":
		case "extracting":
		case "extracted":
		case "installing":
		case "removing":
			return "text-muted-foreground";
		default:
			assertNever(status);
	}
}

function packageApplyProgressKindFromOperation(
	operation: RequestedOperation,
): PackageApplyProgressKind {
	switch (operation.type) {
		case "remove":
		case "bulkRemoved":
			return "remove";
		case "reinstallAll":
		case "bulkReinstalled":
			return "reinstall";
		case "install":
		case "upgradeAll":
		case "resolve":
		case "bulkInstalled":
			return "install";
		default:
			assertNever(operation);
	}
}

function packageApplyProgressStepCount(kind: PackageApplyProgressKind) {
	switch (kind) {
		case "install":
		case "reinstall":
			return 3;
		case "remove":
			return 1;
		default:
			assertNever(kind);
	}
}

function isPackageApplyProgressTarget(
	kind: PackageApplyProgressKind,
	change: TauriPackageChange,
) {
	switch (kind) {
		case "install":
			return "InstallNew" in change;
		case "remove":
			return "Remove" in change;
		case "reinstall":
			return "InstallNew" in change;
		default:
			assertNever(kind);
	}
}

function completedPackageApplyProgressTitle(kind: PackageApplyProgressKind) {
	switch (kind) {
		case "install":
			return tc("projects:manage:progress:install completed");
		case "remove":
			return tc("projects:manage:progress:remove completed");
		case "reinstall":
			return tc("projects:manage:progress:reinstall completed");
		default:
			assertNever(kind);
	}
}

const TypographyItem = ({ children }: { children: React.ReactNode }) => (
	<div className={"p-3"}>
		<p className={"font-normal"}>{children}</p>
	</div>
);

function ProjectChangesDialog({
	changes,
	existingPackages,
	dialog,
}: {
	changes: TauriPendingProjectChanges;
	existingPackages?: [string, TauriBasePackageInfo][];
	dialog: DialogContext<boolean>;
}) {
	const versionConflicts = changes.conflicts.filter(
		([_, c]) => c.packages.length > 0,
	);
	const unityConflicts = changes.conflicts.filter(([_, c]) => c.unity_conflict);
	const unlockedConflicts = changes.conflicts.flatMap(
		([_, c]) => c.unlocked_names,
	);

	const existingPackageMap = new Map(existingPackages ?? []);

	const categorizedChanges = changes.package_changes.map(([pkgId, change]) =>
		categorizeChange(pkgId, change, existingPackageMap),
	);
	categorizedChanges.sort(keyComparator("packageId"));
	const groupedChanges = Array.from(groupBy(categorizedChanges, (c) => c.type));
	groupedChanges.sort(keyComparator(0));

	const installingPackageById = new Map(
		changes.package_changes
			.map(([id, change]) =>
				"InstallNew" in change ? ([id, change.InstallNew] as const) : undefined,
			)
			.filter((x) => x != null),
	);

	function getPackageDisplayName(id: string) {
		return (
			installingPackageById.get(id)?.display_name ??
			existingPackageMap.get(id)?.display_name ??
			id
		);
	}

	const breakingChanges = groupedChanges.some(
		([a]) => a === PackageChangeCategory.UpgradeMajor,
	);

	const incompatibility = changes.conflicts.length !== 0;

	const hasPackageRemoval = groupedChanges.some(
		([category]) =>
			category === PackageChangeCategory.UninstallRequested ||
			category === PackageChangeCategory.UninstallUnused ||
			category === PackageChangeCategory.UninstallLegacy,
	);
	const hasLegacyRemoval =
		changes.remove_legacy_files.length > 0 ||
		changes.remove_legacy_folders.length > 0;
	const hasUnlockedConflictRemoval = unlockedConflicts.length > 0;
	const isDestructiveChange =
		hasPackageRemoval || hasLegacyRemoval || hasUnlockedConflictRemoval;

	return (
		<div className={"contents whitespace-normal"}>
			<DialogHeader>
				<DialogTitle>{tc("projects:manage:button:apply changes")}</DialogTitle>
				<DialogDescription>
					<p>{tc("projects:manage:dialog:confirm changes description")}</p>
					{breakingChanges && (
						<div
							className={
								"flex border border-solid border-warning mt-3 py-2 me-1.5"
							}
						>
							<CircleAlert
								className={"text-warning self-center mx-2 shrink-0"}
							/>
							<p>{tc("projects:manage:dialog:note breaking changes")}</p>
						</div>
					)}
					{incompatibility && (
						<div
							className={
								"flex border border-solid border-warning mt-3 py-2 me-1.5"
							}
						>
							<CircleAlert
								className={"text-warning self-center mx-2 shrink-0"}
							/>
							<p>{tc("projects:manage:dialog:note incompatibility")}</p>
						</div>
					)}
				</DialogDescription>
			</DialogHeader>
			<div className="overflow-hidden flex">
				<ScrollArea
					type="always"
					className={"w-full"}
					scrollBarClassName={"bg-background pb-2.5"}
				>
					<div className="pr-2 overflow-x-hidden">
						<div className={"flex flex-col gap-1 p-2"}>
							{groupedChanges.map(([category, changes], index) => {
								return (
									<Fragment key={category}>
										{index !== 0 && <hr />}
										{changes.map((change) => (
											<PackageChange key={change.packageId} change={change} />
										))}
									</Fragment>
								);
							})}
						</div>
						{versionConflicts.length > 0 ? (
							<>
								<p className={"text-destructive"}>
									{tc("projects:manage:dialog:package version conflicts", {
										count: versionConflicts.length,
									})}
								</p>
								<div className={"flex flex-col gap-1 p-2"}>
									{versionConflicts.map(([pkgId, conflict]) => {
										return (
											<TypographyItem key={pkgId}>
												{tc("projects:manage:dialog:conflicts with", {
													pkg: getPackageDisplayName(pkgId),
													other: conflict.packages
														.map((p) => getPackageDisplayName(p))
														.join(", "),
												})}
											</TypographyItem>
										);
									})}
								</div>
							</>
						) : null}
						{unityConflicts.length > 0 ? (
							<>
								<p className={"text-destructive"}>
									{tc("projects:manage:dialog:unity version conflicts", {
										count: unityConflicts.length,
									})}
								</p>
								<div className={"flex flex-col gap-1 p-2"}>
									{unityConflicts.map(([pkgId, _]) => (
										<TypographyItem key={pkgId}>
											{tc(
												"projects:manage:dialog:package not supported your unity",
												{
													pkg: getPackageDisplayName(pkgId),
												},
											)}
										</TypographyItem>
									))}
								</div>
							</>
						) : null}
						{changes.remove_legacy_files.length > 0 ||
						changes.remove_legacy_folders.length > 0 ? (
							<>
								<p className={"text-destructive"}>
									{tc(
										"projects:manage:dialog:files and directories are removed as legacy",
									)}
								</p>
								<div className={"flex flex-col gap-1 p-2"}>
									{changes.remove_legacy_files.map((f) => (
										<TypographyItem key={f}>{f}</TypographyItem>
									))}
									{changes.remove_legacy_folders.map((f) => (
										<TypographyItem key={f}>{f}</TypographyItem>
									))}
								</div>
							</>
						) : null}
						{unlockedConflicts.length > 0 ? (
							<>
								<p className={"text-destructive"}>
									{tc(
										"projects:manage:dialog:packages installed in the following directories will be removed",
									)}
								</p>
								<div className={"flex flex-col gap-1 p-2"}>
									{unlockedConflicts.map((f) => (
										<TypographyItem key={f}>{f}</TypographyItem>
									))}
								</div>
							</>
						) : null}
					</div>
				</ScrollArea>
			</div>
			<DialogFooter>
				<Button onClick={() => dialog.close(false)} className="mr-1">
					{tc("general:button:cancel")}
				</Button>
				<DelayedButton
					onClick={() => dialog.close(true)}
					variant={isDestructiveChange ? "destructive" : "emphasis"}
					delay={isDestructiveChange ? 1000 : 0}
				>
					{tc("projects:manage:button:apply")}
				</DelayedButton>
			</DialogFooter>
		</div>
	);
}

function PackageChange({
	change,
}: {
	change: PackageChangeDisplayInformation;
}) {
	switch (change.type) {
		case PackageChangeCategory.UpgradeMajor:
			return (
				<div className={"flex items-center p-3 justify-between bg-warning/10"}>
					<p className={"font-normal"}>
						{tc("projects:manage:dialog:upgrade package", {
							name: change.displayName,
							previousVersion: toVersionString(change.previousVersion),
							version: toVersionString(change.version),
						})}
						<span className={"text-warning"}>
							{"\u200B"}
							<CircleAlert
								className={
									"inline px-1 size-5 -mt-0.5 box-content align-middle"
								}
							/>
							{tc("projects:manage:dialog:breaking changes")}
						</span>
					</p>
					<ChangelogButton url={change.changelogUrl} />
				</div>
			);
		case PackageChangeCategory.Upgrade:
			return (
				<div className={"flex items-center p-3 justify-between"}>
					<p className={"font-normal"}>
						{tc("projects:manage:dialog:upgrade package", {
							name: change.displayName,
							previousVersion: toVersionString(change.previousVersion),
							version: toVersionString(change.version),
						})}
					</p>
					<ChangelogButton url={change.changelogUrl} />
				</div>
			);
		case PackageChangeCategory.Downgrade:
			return (
				<div className={"flex items-center p-3 justify-between"}>
					<p className={"font-normal"}>
						{tc("projects:manage:dialog:downgrade package", {
							name: change.displayName,
							previousVersion: toVersionString(change.previousVersion),
							version: toVersionString(change.version),
						})}
					</p>
					<ChangelogButton url={change.changelogUrl} />
				</div>
			);
		case PackageChangeCategory.InstallNew:
			return (
				<div className={"flex items-center p-3 justify-between"}>
					<p className={"font-normal"}>
						{tc("projects:manage:dialog:install package", {
							name: change.displayName,
							version: toVersionString(change.version),
						})}
					</p>
					<ChangelogButton url={change.changelogUrl} />
				</div>
			);
		case PackageChangeCategory.UninstallRequested:
			return (
				<div className={"flex items-center p-3 justify-between"}>
					<p className={"font-normal"}>
						{tc("projects:manage:dialog:uninstall package as requested", {
							name: change.displayName,
						})}
					</p>
				</div>
			);
		case PackageChangeCategory.UninstallUnused:
			return (
				<div className={"flex items-center p-3 justify-between"}>
					<p className={"font-normal"}>
						{tc("projects:manage:dialog:uninstall package as unused", {
							name: change.displayName,
						})}
					</p>
				</div>
			);
		case PackageChangeCategory.UninstallLegacy:
			return (
				<div className={"flex items-center p-3 justify-between"}>
					<p className={"font-normal"}>
						{tc("projects:manage:dialog:uninstall package as legacy", {
							name: change.displayName,
						})}
					</p>
				</div>
			);
		case PackageChangeCategory.Reinstall:
			return (
				<div className={"flex items-center p-3 justify-between"}>
					<p className={"font-normal select-text"}>
						{tc("projects:manage:dialog:reinstall package", {
							name: change.displayName,
							version: toVersionString(change.version),
						})}
					</p>
					<ChangelogButton url={change.changelogUrl} />
				</div>
			);
	}
}

enum PackageChangeCategory {
	InstallNew = 0,
	UpgradeMajor = 1,
	Upgrade = 2,
	Downgrade = 3,
	UninstallRequested = 4,
	UninstallUnused = 5,
	UninstallLegacy = 6,
	Reinstall = 7,
}

type PackageChangeDisplayInformation = {
	packageId: string;
	displayName: string;
} & (
	| {
			type: PackageChangeCategory.UpgradeMajor;
			version: TauriVersion;
			previousVersion: TauriVersion;
			changelogUrl: string | null;
	  }
	| {
			type: PackageChangeCategory.Upgrade;
			version: TauriVersion;
			previousVersion: TauriVersion;
			changelogUrl: string | null;
	  }
	| {
			type: PackageChangeCategory.Downgrade;
			version: TauriVersion;
			previousVersion: TauriVersion;
			changelogUrl: string | null;
	  }
	| {
			type: PackageChangeCategory.Reinstall;
			version: TauriVersion;
			changelogUrl: string | null;
	  }
	| {
			type: PackageChangeCategory.InstallNew;
			version: TauriVersion;
			changelogUrl: string | null;
	  }
	| {
			type: PackageChangeCategory.UninstallRequested;
	  }
	| {
			type: PackageChangeCategory.UninstallUnused;
	  }
	| {
			type: PackageChangeCategory.UninstallLegacy;
	  }
);

function categorizeChange(
	pkgId: string,
	change: TauriPackageChange,
	installedPackages: Map<string, TauriBasePackageInfo>,
): PackageChangeDisplayInformation {
	if (change.InstallNew !== undefined) {
		const name = change.InstallNew.display_name ?? change.InstallNew.name;

		const installed = installedPackages.get(pkgId);
		if (installed == null) {
			return {
				packageId: pkgId,
				displayName: name,
				type: PackageChangeCategory.InstallNew,
				version: change.InstallNew.version,
				changelogUrl: change.InstallNew.changelog_url,
			};
		} else {
			const compare = compareVersion(
				installed.version,
				change.InstallNew.version,
			);
			switch (compare) {
				case 1:
					return {
						packageId: pkgId,
						displayName: name,
						type: PackageChangeCategory.Downgrade,
						version: change.InstallNew.version,
						previousVersion: installed.version,
						changelogUrl: change.InstallNew.changelog_url,
					};
				case 0:
					return {
						packageId: pkgId,
						displayName: name,
						type: PackageChangeCategory.Reinstall,
						version: change.InstallNew.version,
						changelogUrl: change.InstallNew.changelog_url,
					};
				case -1:
					if (
						isUpgradingMajorly(
							pkgId,
							installed.version,
							change.InstallNew.version,
						)
					) {
						return {
							packageId: pkgId,
							displayName: name,
							type: PackageChangeCategory.UpgradeMajor,
							version: change.InstallNew.version,
							previousVersion: installed.version,
							changelogUrl: change.InstallNew.changelog_url,
						};
					} else {
						return {
							packageId: pkgId,
							displayName: name,
							type: PackageChangeCategory.Upgrade,
							version: change.InstallNew.version,
							previousVersion: installed.version,
							changelogUrl: change.InstallNew.changelog_url,
						};
					}
			}
		}
	} else {
		const name = installedPackages.get(pkgId)?.display_name ?? pkgId;
		switch (change.Remove) {
			case "Requested":
				return {
					packageId: pkgId,
					displayName: name,
					type: PackageChangeCategory.UninstallRequested,
				};
			case "Legacy":
				return {
					packageId: pkgId,
					displayName: name,
					type: PackageChangeCategory.UninstallLegacy,
				};
			case "Unused":
				return {
					packageId: pkgId,
					displayName: name,
					type: PackageChangeCategory.UninstallUnused,
				};
		}
	}
}

function isUpgradingMajorly(
	pkgId: string,
	prevVersion: TauriVersion,
	newVersion: TauriVersion,
): boolean {
	function firstNonZeroVersionNum(version: TauriVersion): number {
		if (version.major !== 0) return version.major;
		if (version.minor !== 0) return version.minor;
		return version.patch;
	}

	// generic case: non-zero first version number will be the major version
	if (
		firstNonZeroVersionNum(prevVersion) !== firstNonZeroVersionNum(newVersion)
	) {
		return true;
	}
	// Special case: VRChat SDK uses Branding.Breaking.Bumps.
	// Therefore the second number bump means major version bump.
	// See https://vcc.docs.vrchat.com/vpm/packages/#brandingbreakingbumps
	// See https://feedback.vrchat.com/sdk-bug-reports/p/feedback-please-dont-make-vrcsdk-to-4x-unless-as-big-breaking-changes-as-2-to-3
	if (
		pkgId === "com.vrchat.avatars" ||
		pkgId === "com.vrchat.worlds" ||
		pkgId === "com.vrchat.base"
	) {
		if (prevVersion.minor !== newVersion.minor) {
			return true;
		}
	}

	// No conditions met so it's not major bump
	return false;
}

function ChangelogButton({ url }: { url?: string | null }) {
	if (url == null) return null;
	try {
		const parsed = new URL(url);
		if (parsed.protocol === "http:" || parsed.protocol === "https:") {
			return (
				<Button
					className={"ml-1 px-2"}
					size={"sm"}
					onClick={() => commands.utilOpenUrl(url)}
				>
					<ExternalLink>
						{tc("projects:manage:button:see changelog")}
					</ExternalLink>
				</Button>
			);
		}
	} catch {}

	return null;
}

function MissingDependenciesDialog({
	dependencies,
	dialog,
}: {
	dependencies: [pkg: string, range: string][];
	dialog: DialogContext<void>;
}) {
	return (
		<div>
			<DialogTitle className={"text-destructive"}>
				<CircleAlert className="size-6 inline" />{" "}
				{tc("projects:manage:dialog:missing dependencies")}
			</DialogTitle>
			<div>
				<p className={"whitespace-normal"}>
					{tc("projects:manage:dialog:missing dependencies description")}
				</p>
				<ul className={"list-disc ml-4 mt-2"}>
					{dependencies.map(([dep, range]) => (
						<li key={dep}>
							{dep} version {range}
						</li>
					))}
				</ul>
			</div>
			<DialogFooter>
				<Button onClick={() => dialog.close()}>
					{tc("general:button:close")}
				</Button>
			</DialogFooter>
		</div>
	);
}
