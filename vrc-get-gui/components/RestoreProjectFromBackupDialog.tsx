import type { QueryClient } from "@tanstack/react-query";
import { Minimize2, RefreshCw } from "lucide-react";
import { useId, useState, useSyncExternalStore } from "react";
import { VStack } from "@/components/layout";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogFooter,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { assertNever } from "@/lib/assert-never";
import {
	commands,
	type TauriRestoreProjectFromBackupProgress,
	type TauriRestoreProjectFromBackupResult,
} from "@/lib/bindings";
import { callAsyncCommand } from "@/lib/call-async-command";
import { type DialogContext, showDialog } from "@/lib/dialog";
import { tc, tt } from "@/lib/i18n";
import { pathSeparator } from "@/lib/os";
import {
	ProjectNameCheckResult,
	useProjectNameCheck,
} from "@/lib/project-name-check";
import {
	emitProjectProgressTasksChanged,
	registerProjectProgressTask,
} from "@/lib/project-progress-task";
import {
	toastError,
	toastNormal,
	toastSuccess,
	toastThrownError,
} from "@/lib/toast";

type RestoreBackupProgressState = {
	progress: TauriRestoreProjectFromBackupProgress;
	minimized: boolean;
	cancelRequested: boolean;
	cancel: () => void;
	promise: Promise<TauriRestoreProjectFromBackupResult | "cancelled">;
};

let restoreBackupProgressState: RestoreBackupProgressState | null = null;
let restoreBackupPreparationActive = false;
const restoreBackupProgressListeners = new Set<() => void>();

function emitRestoreBackupProgress() {
	for (const listener of restoreBackupProgressListeners) listener();
}

function setRestoreBackupProgressState(
	state: RestoreBackupProgressState | null,
) {
	restoreBackupProgressState = state;
	emitRestoreBackupProgress();
	emitProjectProgressTasksChanged();
}

function subscribeRestoreBackupProgress(listener: () => void) {
	restoreBackupProgressListeners.add(listener);
	return () => restoreBackupProgressListeners.delete(listener);
}

function getRestoreBackupProgressSnapshot() {
	return restoreBackupProgressState;
}

function minimizeRestoreBackupProgress() {
	if (restoreBackupProgressState == null) return;
	setRestoreBackupProgressState({
		...restoreBackupProgressState,
		minimized: true,
	});
}

function restoreRestoreBackupProgress() {
	if (restoreBackupProgressState == null) return;
	setRestoreBackupProgressState({
		...restoreBackupProgressState,
		minimized: false,
	});
}

function cancelRestoreBackupProgress() {
	if (restoreBackupProgressState == null) return;
	setRestoreBackupProgressState({
		...restoreBackupProgressState,
		cancelRequested: true,
		minimized: false,
	});
	restoreBackupProgressState.cancel();
}

registerProjectProgressTask("restore", {
	isActive: () => restoreBackupProgressState != null,
	isMinimized: () => restoreBackupProgressState?.minimized === true,
	restore: restoreRestoreBackupProgress,
	renderRestoreButton: () => (
		<>
			<RefreshCw className="size-4 animate-spin" />
			{tc("projects:dialog:restore backup restore")}
		</>
	),
});

export async function restoreProjectFromBackup(queryClient: QueryClient) {
	if (restoreBackupProgressState != null) {
		restoreRestoreBackupProgress();
		toastNormal(tc("projects:toast:restore already running"));
		return;
	}
	if (restoreBackupPreparationActive) return;
	restoreBackupPreparationActive = true;

	try {
		const selection = await commands.environmentPickProjectBackupForRestore();
		switch (selection.type) {
			case "NoFileSelected":
				return;
			case "InvalidSelection":
				toastError(tt("general:toast:invalid directory"));
				return;
			case "Successful":
				break;
			default:
				assertNever(selection);
		}

		let projectName: string | null;
		{
			using dialog = showDialog();
			projectName = await dialog.ask(RestoreProjectNameDialog, {
				projectLocation: selection.project_location,
				initialProjectName: selection.project_name,
			});
		}
		if (projectName == null) return;

		const [cancel, commandPromise] = callAsyncCommand(
			commands.environmentRestoreProjectFromBackup,
			[selection.backup_path, projectName],
			(progress) => {
				const prev = restoreBackupProgressState;
				if (prev == null) return;
				if (prev.progress.proceed > progress.proceed) return;
				setRestoreBackupProgressState({
					...prev,
					progress,
				});
			},
		);

		const promise = commandPromise.finally(() => {
			setRestoreBackupProgressState(null);
		});

		setRestoreBackupProgressState({
			progress: {
				proceed: 0,
				total: 1,
				last_proceed: tt("projects:dialog:restoring backup..."),
			},
			minimized: false,
			cancelRequested: false,
			cancel,
			promise,
		});

		const result = await promise;

		switch (result) {
			case "cancelled":
				toastNormal(tt("projects:toast:restore canceled"));
				break;
			case "InvalidSelection":
				toastError(tt("general:toast:invalid directory"));
				break;
			case "AlreadyExists":
			case "AlreadyAdded":
				toastError(tt("projects:toast:project already exists"));
				break;
			case "Successful":
				toastSuccess(tt("projects:toast:project restored from backup"));
				break;
			default:
				assertNever(result);
		}

		await queryClient.invalidateQueries({
			queryKey: ["environmentProjects"],
		});
	} catch (e) {
		toastThrownError(e);
	} finally {
		restoreBackupPreparationActive = false;
	}
}

function RestoreProjectNameDialog({
	dialog,
	projectLocation,
	initialProjectName,
}: {
	dialog: DialogContext<string | null>;
	projectLocation: string;
	initialProjectName: string;
}) {
	const [projectNameRaw, setProjectName] = useState(initialProjectName);
	const projectName = projectNameRaw.trim();
	const projectNameCheckState = useProjectNameCheck(
		projectLocation,
		projectName,
	);
	const projectNameInputId = useId();
	const badProjectName = ["AlreadyExists", "InvalidNameForFolderName"].includes(
		projectNameCheckState,
	);
	const canRestore = projectNameCheckState !== "checking" && !badProjectName;

	return (
		<>
			<DialogTitle>{tc("projects:dialog:restore backup header")}</DialogTitle>
			<div>
				<VStack>
					<label htmlFor={projectNameInputId}>{tc("general:name")}</label>
					<Input
						id={projectNameInputId}
						autoFocus
						value={projectNameRaw}
						onChange={(event) => setProjectName(event.target.value)}
					/>
					<small className={"whitespace-normal"}>
						{tc(
							"projects:hint:path of creating project",
							{
								path: `${projectLocation}${pathSeparator()}${projectName}`,
							},
							{
								components: {
									path: (
										<span
											className={
												"p-0.5 font-path whitespace-pre bg-secondary text-secondary-foreground"
											}
										/>
									),
								},
							},
						)}
					</small>
					<ProjectNameCheckResult
						projectNameCheckState={projectNameCheckState}
						readyLabel={tc("projects:hint:restore project ready")}
					/>
				</VStack>
			</div>
			<DialogFooter className={"gap-2"}>
				<Button onClick={() => dialog.close(null)}>
					{tc("general:button:cancel")}
				</Button>
				<Button
					disabled={!canRestore}
					onClick={() => dialog.close(projectName)}
				>
					{tc("projects:restore from backup")}
				</Button>
			</DialogFooter>
		</>
	);
}

export function RestoreBackupProgressHost() {
	const state = useSyncExternalStore(
		subscribeRestoreBackupProgress,
		getRestoreBackupProgressSnapshot,
	);

	if (state == null) return null;

	return (
		<Dialog
			open={!state.minimized}
			onOpenChange={(open) => {
				if (!open) minimizeRestoreBackupProgress();
			}}
		>
			<DialogContent
				onEscapeKeyDown={(event) => {
					event.preventDefault();
					minimizeRestoreBackupProgress();
				}}
			>
				<DialogTitle>{tc("projects:dialog:restore backup header")}</DialogTitle>
				<div>
					{state.cancelRequested ? (
						<p>{tc("projects:manage:progress:cancelling")}</p>
					) : (
						<>
							<p>{tc("projects:dialog:restoring backup...")}</p>
							<p>
								{tc("projects:dialog:proceed k/n", {
									count: state.progress.proceed,
									total: state.progress.total,
								})}
							</p>
							<p className={"overflow-hidden w-full whitespace-pre"}>
								{state.progress.last_proceed}
							</p>
							<Progress
								value={state.progress.proceed}
								max={state.progress.total}
							/>
							<p>{tc("projects:do not close")}</p>
						</>
					)}
				</div>
				<DialogFooter className={"gap-2"}>
					<Button
						disabled={state.cancelRequested}
						onClick={() => cancelRestoreBackupProgress()}
					>
						{state.cancelRequested
							? tc("projects:manage:progress:cancelling")
							: tc("general:button:cancel")}
					</Button>
					<Button className="gap-2" onClick={minimizeRestoreBackupProgress}>
						<Minimize2 className="size-4" />
						{tc("projects:manage:progress:minimize")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
