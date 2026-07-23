import { useSyncExternalStore } from "react";
import { Button } from "@/components/ui/button";
import {
	getMinimizedProjectProgressTasks,
	getProjectProgressTasksSnapshot,
	subscribeProjectProgressTasks,
} from "@/lib/project-progress-task";

export function ProjectProgressTaskTray() {
	useSyncExternalStore(
		subscribeProjectProgressTasks,
		getProjectProgressTasksSnapshot,
	);

	const tasks = getMinimizedProjectProgressTasks();
	if (tasks.length === 0) return null;

	return (
		<div className="fixed bottom-4 right-4 z-50 flex flex-col items-end gap-2">
			{tasks.map(([kind, task]) => (
				<Button key={kind} className="gap-2 shadow-2xl" onClick={task.restore}>
					{task.renderRestoreButton()}
				</Button>
			))}
		</div>
	);
}
