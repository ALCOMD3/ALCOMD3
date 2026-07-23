import { useMutation } from "@tanstack/react-query";
import type { NavigateFn } from "@tanstack/react-router";
import { Minimize2, RefreshCw } from "lucide-react";
import { useState, useSyncExternalStore } from "react";
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
import { commands, type TauriCopyProjectProgress } from "@/lib/bindings";
import { callAsyncCommand } from "@/lib/call-async-command";
import { type DialogContext, showDialog } from "@/lib/dialog";
import { tc, tt } from "@/lib/i18n";
import { directoryFromPath, nameFromPath, pathSeparator } from "@/lib/os";
import {
	ProjectNameCheckResult,
	useProjectNameCheck,
} from "@/lib/project-name-check";
import {
	emitProjectProgressTasksChanged,
	registerProjectProgressTask,
} from "@/lib/project-progress-task";
import { queryClient } from "@/lib/query-client";
import {
	toastError,
	toastNormal,
	toastSuccess,
	toastThrownError,
} from "@/lib/toast";

export async function copyProject(existingPath: string, navigate?: NavigateFn) {
	if (restoreCopyProjectProgressWithToast()) {
		return;
	}

	let newPath: string | null;
	{
		using dialog = showDialog();
		newPath = await dialog.ask(CopyProjectNameDialog, {
			projectPath: existingPath,
		});
	}
	if (newPath == null) return; // cancelled

	const result = await startCopyProjectProgress(existingPath, newPath);
	if (result === "cancelled") return;

	await Promise.all([
		queryClient.invalidateQueries({
			queryKey: ["projectDetails", existingPath],
		}),
		queryClient.invalidateQueries({
			queryKey: ["environmentProjects"],
		}),
	]);

	await navigate?.({
		replace: true,
		to: "/projects/manage",
		search: { projectPath: newPath },
	});
}

type CopyProjectProgressState = {
	projectPath: string;
	newProjectPath: string;
	progress: TauriCopyProjectProgress;
	minimized: boolean;
	cancelRequested: boolean;
	cancel: () => void;
	promise: Promise<string | "cancelled">;
};

const initialCopyProgress: TauriCopyProjectProgress = {
	proceed: 0,
	total: 1,
	last_proceed: "Collecting files...",
};

let copyProjectProgressState: CopyProjectProgressState | null = null;
const copyProjectProgressListeners = new Set<() => void>();

function emitCopyProjectProgress() {
	for (const listener of copyProjectProgressListeners) listener();
}

function setCopyProjectProgressState(state: CopyProjectProgressState | null) {
	copyProjectProgressState = state;
	emitCopyProjectProgress();
	emitProjectProgressTasksChanged();
}

function subscribeCopyProjectProgress(listener: () => void) {
	copyProjectProgressListeners.add(listener);
	return () => copyProjectProgressListeners.delete(listener);
}

function getCopyProjectProgressSnapshot() {
	return copyProjectProgressState;
}

function updateCopyProjectProgress(progress: TauriCopyProjectProgress) {
	if (copyProjectProgressState == null) return;
	if (copyProjectProgressState.progress.proceed > progress.proceed) return;
	setCopyProjectProgressState({
		...copyProjectProgressState,
		progress,
	});
}

function minimizeCopyProjectProgress() {
	if (copyProjectProgressState == null) return;
	setCopyProjectProgressState({
		...copyProjectProgressState,
		minimized: true,
	});
}

function restoreCopyProjectProgress() {
	if (copyProjectProgressState == null) return;
	setCopyProjectProgressState({
		...copyProjectProgressState,
		minimized: false,
	});
}

registerProjectProgressTask("copy", {
	isActive: () => copyProjectProgressState != null,
	isMinimized: () => copyProjectProgressState?.minimized === true,
	restore: restoreCopyProjectProgress,
	renderRestoreButton: () => (
		<>
			<RefreshCw className="size-4 animate-spin" />
			{tc("projects:manage:progress:restore")}
		</>
	),
});

function cancelCopyProjectProgress() {
	if (copyProjectProgressState == null) return;
	setCopyProjectProgressState({
		...copyProjectProgressState,
		cancelRequested: true,
		minimized: false,
	});
	copyProjectProgressState.cancel();
}

function restoreCopyProjectProgressWithToast() {
	if (copyProjectProgressState == null) return false;
	restoreCopyProjectProgress();
	toastNormal(tc("projects:toast:copy already running"));
	return true;
}

function startCopyProjectProgress(projectPath: string, newProjectPath: string) {
	if (restoreCopyProjectProgressWithToast()) {
		return Promise.resolve("cancelled" as const);
	}

	const [cancel, commandPromise] = callAsyncCommand(
		commands.environmentCopyProject,
		[projectPath, newProjectPath],
		updateCopyProjectProgress,
	);

	const promise = commandPromise
		.then((result) => {
			if (result === "cancelled") {
				toastNormal(tc("projects:toast:copy canceled"));
			} else {
				toastSuccess(
					tc("projects:toast:successfully copied project", {
						name: nameFromPath(projectPath),
					}),
				);
			}
			setCopyProjectProgressState(null);
			return result;
		})
		.catch((error) => {
			setCopyProjectProgressState(null);
			throw error;
		});

	setCopyProjectProgressState({
		projectPath,
		newProjectPath,
		progress: initialCopyProgress,
		minimized: false,
		cancelRequested: false,
		cancel,
		promise,
	});

	return promise;
}

export function CopyProjectProgressHost() {
	const state = useSyncExternalStore(
		subscribeCopyProjectProgress,
		getCopyProjectProgressSnapshot,
	);

	if (state == null) return null;

	const oldName = nameFromPath(state.projectPath);

	return (
		<Dialog
			open={!state.minimized}
			onOpenChange={(open) => {
				if (!open) minimizeCopyProjectProgress();
			}}
		>
			<DialogContent
				onEscapeKeyDown={(event) => {
					event.preventDefault();
					minimizeCopyProjectProgress();
				}}
			>
				<DialogTitle>
					{tc("projects:dialog:copy project", { name: oldName })}
				</DialogTitle>
				<div>
					{state.cancelRequested ? (
						<p>{tc("projects:manage:progress:cancelling")}</p>
					) : (
						<>
							<p>{tc("projects:dialog:copying...")}</p>
							<p>
								{tc("projects:dialog:proceed k/n", {
									count: state.progress.proceed,
									total: state.progress.total,
								})}
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
						onClick={() => cancelCopyProjectProgress()}
					>
						{state.cancelRequested
							? tc("projects:manage:progress:cancelling")
							: tc("general:button:cancel")}
					</Button>
					<Button className="gap-2" onClick={minimizeCopyProjectProgress}>
						<Minimize2 className="size-4" />
						{tc("projects:manage:progress:minimize")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

function CopyProjectNameDialog({
	dialog,
	projectPath,
}: {
	dialog: DialogContext<string | null>;
	projectPath: string;
}) {
	const oldName = nameFromPath(projectPath);
	const [projectNameRaw, setProjectName] = useState(`${oldName}-Copy`);
	const projectName = projectNameRaw.trim();
	const [projectLocation, setProjectLocation] = useState(
		directoryFromPath(projectPath),
	);
	const projectNameCheckState = useProjectNameCheck(
		projectLocation,
		projectName,
	);

	const usePickProjectLocationPath = useMutation({
		mutationFn: () => commands.utilPickDirectory(projectLocation),
		onSuccess: (result) => {
			switch (result.type) {
				case "NoFolderSelected":
					// no-op
					break;
				case "InvalidSelection":
					toastError(tt("general:toast:invalid directory"));
					break;
				case "Successful":
					setProjectLocation(result.new_path);
					break;
				default:
					assertNever(result);
			}
		},
		onError: (e) => {
			console.error(e);
			toastThrownError(e);
		},
	});

	const createProject = async () => {
		dialog.close(`${projectLocation}${pathSeparator()}${projectName}`);
	};

	const badProjectName = ["AlreadyExists", "InvalidNameForFolderName"].includes(
		projectNameCheckState,
	);

	const canCreateProject =
		projectNameCheckState !== "checking" && !badProjectName;

	return (
		<>
			<DialogTitle>
				{tc("projects:dialog:copy project", { name: oldName })}
			</DialogTitle>
			<div>
				<VStack>
					<Input
						value={projectNameRaw}
						onChange={(e) => setProjectName(e.target.value)}
					/>
					<div className={"flex gap-1 items-center"}>
						<Input className="flex-auto" value={projectLocation} disabled />
						<Button
							className="flex-none px-4"
							onClick={() => usePickProjectLocationPath.mutate()}
						>
							{tc("general:button:select")}
						</Button>
					</div>
					<small className={"whitespace-normal"}>
						{tc(
							"projects:hint:path of creating project",
							{ path: `${projectLocation}${pathSeparator()}${projectName}` },
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
					/>
				</VStack>
			</div>
			<DialogFooter className={"gap-2"}>
				<Button onClick={() => dialog.close(null)}>
					{tc("general:button:cancel")}
				</Button>
				<Button onClick={createProject} disabled={!canCreateProject}>
					{tc("projects:button:create")}
				</Button>
			</DialogFooter>
		</>
	);
}
