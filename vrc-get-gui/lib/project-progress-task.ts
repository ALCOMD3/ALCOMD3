import type { ReactNode } from "react";

export type ProjectProgressTaskKind = "backup" | "copy" | "restore";

type ProjectProgressTask = {
	isActive: () => boolean;
	isMinimized: () => boolean;
	restore: () => void;
	renderRestoreButton: () => ReactNode;
};

const projectProgressTasks = new Map<
	ProjectProgressTaskKind,
	ProjectProgressTask
>();
const projectProgressTaskListeners = new Set<() => void>();
let projectProgressTasksVersion = 0;

export function registerProjectProgressTask(
	kind: ProjectProgressTaskKind,
	task: ProjectProgressTask,
) {
	projectProgressTasks.set(kind, task);
	emitProjectProgressTasksChanged();
}

export function emitProjectProgressTasksChanged() {
	projectProgressTasksVersion++;
	for (const listener of projectProgressTaskListeners) {
		listener();
	}
}

export function subscribeProjectProgressTasks(listener: () => void) {
	projectProgressTaskListeners.add(listener);
	return () => projectProgressTaskListeners.delete(listener);
}

export function getProjectProgressTasksSnapshot() {
	return projectProgressTasksVersion;
}

export function getMinimizedProjectProgressTasks() {
	return [...projectProgressTasks.entries()].filter(
		([_, task]) => task.isActive() && task.isMinimized(),
	);
}

export function restoreProjectProgressTask(kind: ProjectProgressTaskKind) {
	projectProgressTasks.get(kind)?.restore();
}
