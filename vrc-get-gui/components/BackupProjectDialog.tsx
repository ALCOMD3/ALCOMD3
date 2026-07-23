import { useQuery } from "@tanstack/react-query";
import { useDebounce } from "@uidotdev/usehooks";
import { Minimize2, RefreshCw } from "lucide-react";
import type React from "react";
import { useId, useState, useSyncExternalStore } from "react";
import { VStack } from "@/components/layout";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
	Dialog,
	DialogContent,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { assertNever } from "@/lib/assert-never";
import {
	commands,
	type TauriBackupNameCheckResult,
	type TauriCreateBackupProgress,
} from "@/lib/bindings";
import { callAsyncCommand } from "@/lib/call-async-command";
import { type DialogContext, showDialog } from "@/lib/dialog";
import { tc } from "@/lib/i18n";
import { pathSeparator } from "@/lib/os";
import {
	emitProjectProgressTasksChanged,
	registerProjectProgressTask,
} from "@/lib/project-progress-task";
import { toastNormal, toastSuccess } from "@/lib/toast";

type BackupProgressState = {
	projectPath: string;
	header?: React.ReactNode;
	progress: TauriCreateBackupProgress;
	minimized: boolean;
	cancelRequested: boolean;
	cancel: () => void;
	promise: Promise<null | "cancelled">;
};

type BackupOptions = {
	backupName: string;
	excludeVpmPackagesFromBackup: boolean;
};

const initialBackupProgress: TauriCreateBackupProgress = {
	proceed: 0,
	total: 1,
	last_proceed: "",
};

let backupProgressState: BackupProgressState | null = null;
let backupPreparationActive = false;
const backupProgressListeners = new Set<() => void>();

function emitBackupProgress() {
	for (const listener of backupProgressListeners) listener();
}

function setBackupProgressState(state: BackupProgressState | null) {
	backupProgressState = state;
	emitBackupProgress();
	emitProjectProgressTasksChanged();
}

function subscribeBackupProgress(listener: () => void) {
	backupProgressListeners.add(listener);
	return () => backupProgressListeners.delete(listener);
}

function getBackupProgressSnapshot() {
	return backupProgressState;
}

function updateBackupProgress(progress: TauriCreateBackupProgress) {
	if (backupProgressState == null) return;
	if (backupProgressState.progress.proceed > progress.proceed) return;
	setBackupProgressState({
		...backupProgressState,
		progress,
	});
}

function minimizeBackupProgress() {
	if (backupProgressState == null) return;
	setBackupProgressState({
		...backupProgressState,
		minimized: true,
	});
}

function restoreBackupProgress() {
	if (backupProgressState == null) return;
	setBackupProgressState({
		...backupProgressState,
		minimized: false,
	});
}

registerProjectProgressTask("backup", {
	isActive: () => backupProgressState != null,
	isMinimized: () => backupProgressState?.minimized === true,
	restore: restoreBackupProgress,
	renderRestoreButton: () => (
		<>
			<RefreshCw className="size-4 animate-spin" />
			{tc("projects:dialog:backup restore")}
		</>
	),
});

function cancelBackupProgress() {
	if (backupProgressState == null) return;
	setBackupProgressState({
		...backupProgressState,
		cancelRequested: true,
		minimized: false,
	});
	backupProgressState.cancel();
}

export async function startProjectBackup({
	projectPath,
	header,
}: {
	projectPath: string;
	header?: React.ReactNode;
}) {
	if (backupProgressState != null) {
		restoreBackupProgress();
		toastNormal(tc("projects:toast:backup already running"));
		return "cancelled" as const;
	}
	if (backupPreparationActive) return "cancelled" as const;
	backupPreparationActive = true;

	try {
		const information =
			await commands.projectBackupCreationInformation(projectPath);
		let backupOptions: BackupOptions | null;
		{
			using dialog = showDialog();
			backupOptions = await dialog.ask(BackupNameDialog, {
				header,
				backupDirectory: information.backup_directory,
				initialBackupName: information.default_backup_name,
			});
		}
		if (backupOptions == null) return "cancelled" as const;

		return startProjectBackupProgress({
			projectPath,
			...backupOptions,
			header,
		});
	} finally {
		backupPreparationActive = false;
	}
}

function startProjectBackupProgress({
	projectPath,
	backupName,
	excludeVpmPackagesFromBackup,
	header,
}: {
	projectPath: string;
	backupName: string;
	excludeVpmPackagesFromBackup: boolean;
	header?: React.ReactNode;
}) {
	const [cancel, commandPromise] = callAsyncCommand(
		commands.projectCreateBackup,
		[projectPath, backupName, excludeVpmPackagesFromBackup],
		updateBackupProgress,
	);

	const promise = commandPromise
		.then((result) => {
			if (!header) {
				if (result === "cancelled") {
					toastNormal(tc("projects:toast:backup canceled"));
				} else {
					toastSuccess(tc("projects:toast:backup succeeded"));
				}
			}
			setBackupProgressState(null);
			return result;
		})
		.catch((error) => {
			setBackupProgressState(null);
			throw error;
		});

	setBackupProgressState({
		projectPath,
		header,
		progress: initialBackupProgress,
		minimized: false,
		cancelRequested: false,
		cancel,
		promise,
	});

	return promise;
}

function useBackupNameCheck(
	backupName: string,
): "checking" | TauriBackupNameCheckResult {
	const backupNameDebounced = useDebounce(backupName, 500);
	const check = useQuery({
		queryKey: ["projectCheckBackupName", backupNameDebounced],
		queryFn: () => commands.projectCheckBackupName(backupNameDebounced),
	});

	return backupNameDebounced !== backupName || check.isFetching
		? "checking"
		: (check.data ?? "checking");
}

function BackupNameDialog({
	dialog,
	header,
	backupDirectory,
	initialBackupName,
}: {
	dialog: DialogContext<BackupOptions | null>;
	header?: React.ReactNode;
	backupDirectory: string;
	initialBackupName: string;
}) {
	const [backupNameRaw, setBackupName] = useState(initialBackupName);
	const backupName = backupNameRaw.trim();
	const backupNameCheckState = useBackupNameCheck(backupName);
	const backupNameInputId = useId();
	const excludeVpmPackagesInputId = useId();
	const [excludeVpmPackagesFromBackup, setExcludeVpmPackagesFromBackup] =
		useState(false);
	const canBackup = backupNameCheckState === "Ok";

	return (
		<>
			<DialogTitle>{header ?? tc("projects:dialog:backup header")}</DialogTitle>
			<div>
				<VStack>
					<label htmlFor={backupNameInputId}>{tc("general:name")}</label>
					<Input
						id={backupNameInputId}
						autoFocus
						value={backupNameRaw}
						onChange={(event) => setBackupName(event.target.value)}
					/>
					<small className={"whitespace-normal"}>
						{tc(
							"projects:hint:path of creating backup",
							{
								path: `${backupDirectory}${pathSeparator()}${backupName}.zip`,
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
					<BackupNameValidationMessage state={backupNameCheckState} />
					<div className="mt-2">
						<label
							htmlFor={excludeVpmPackagesInputId}
							className="flex items-center gap-2"
						>
							<Checkbox
								id={excludeVpmPackagesInputId}
								checked={excludeVpmPackagesFromBackup}
								onCheckedChange={(checked) =>
									setExcludeVpmPackagesFromBackup(checked === true)
								}
							/>
							{tc("projects:dialog:backup exclude vpm packages")}
						</label>
						<p className="mt-1 whitespace-normal text-sm text-muted-foreground">
							{tc("projects:dialog:backup exclude vpm packages description")}
						</p>
					</div>
				</VStack>
			</div>
			<DialogFooter className={"gap-2"}>
				<Button onClick={() => dialog.close(null)}>
					{tc("general:button:cancel")}
				</Button>
				<Button
					disabled={!canBackup}
					onClick={() =>
						dialog.close({ backupName, excludeVpmPackagesFromBackup })
					}
				>
					{tc("projects:backup")}
				</Button>
			</DialogFooter>
		</>
	);
}

function BackupNameValidationMessage({
	state,
}: {
	state: "checking" | TauriBackupNameCheckResult;
}) {
	switch (state) {
		case "Ok":
			return (
				<small className={"whitespace-normal text-success"}>
					{tc("projects:hint:backup ready")}
				</small>
			);
		case "InvalidNameForFileName":
			return (
				<small className={"whitespace-normal text-destructive"}>
					{tc("projects:hint:invalid backup name")}
				</small>
			);
		case "AlreadyExists":
			return (
				<small className={"whitespace-normal text-destructive"}>
					{tc("projects:hint:backup already exists")}
				</small>
			);
		case "checking":
			return (
				<small className={"whitespace-normal"}>
					<RefreshCw className={"w-5 h-5 animate-spin"} />
				</small>
			);
		default:
			assertNever(state);
	}
}

export function BackupProjectProgressHost() {
	const state = useSyncExternalStore(
		subscribeBackupProgress,
		getBackupProgressSnapshot,
	);

	if (state == null) return null;

	return (
		<Dialog
			open={!state.minimized}
			onOpenChange={(open) => {
				if (!open) minimizeBackupProgress();
			}}
		>
			<DialogContent
				className="max-h-[calc(100dvh-(var(--spacing)*8))] overflow-y-auto"
				onEscapeKeyDown={(event) => {
					event.preventDefault();
					minimizeBackupProgress();
				}}
			>
				<DialogHeader>
					<DialogTitle>
						{state.header ?? tc("projects:dialog:backup header")}
					</DialogTitle>
				</DialogHeader>
				<div>
					{state.cancelRequested ? (
						<p>{tc("projects:manage:progress:cancelling")}</p>
					) : (
						<>
							<p>{tc("projects:dialog:creating backup...")}</p>
							<p>
								{tc("projects:dialog:proceed k/n", {
									count: state.progress.proceed,
									total: state.progress.total,
								})}
							</p>
							<p className={"overflow-hidden w-full whitespace-pre"}>
								{state.progress.last_proceed || "Collecting files..."}
							</p>
							<Progress
								value={state.progress.proceed}
								max={state.progress.total}
							/>
						</>
					)}
				</div>
				<DialogFooter>
					<Button
						className="mr-1"
						disabled={state.cancelRequested}
						onClick={() => cancelBackupProgress()}
					>
						{state.cancelRequested
							? tc("projects:manage:progress:cancelling")
							: tc("general:button:cancel")}
					</Button>
					<Button className="gap-2" onClick={minimizeBackupProgress}>
						<Minimize2 className="size-4" />
						{tc("projects:manage:progress:minimize")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
