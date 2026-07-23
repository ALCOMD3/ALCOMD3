import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useLocation, useRouter } from "@tanstack/react-router";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { DialogFooter, DialogTitle } from "@/components/ui/dialog";
import { commands } from "@/lib/bindings";
import type { DialogContext } from "@/lib/dialog";
import { tc, tt } from "@/lib/i18n";
import { nameFromPath } from "@/lib/os";
import { toastSuccess, toastThrownError } from "@/lib/toast";

type Project = {
	path: string;
	is_exists: boolean;
};

export function RemoveProjectDialog({
	project,
	dialog,
}: {
	project: Project;
	dialog: DialogContext<boolean>;
}) {
	const queryClient = useQueryClient();
	const router = useRouter();
	const location = useLocation();
	const [lastDirectoryRemoveFailed, setLastDirectoryRemoveFailed] =
		useState(false);
	const [lastErrorMessage, setLastErrorMessage] = useState<string | null>(null);

	const getErrorMessage = (error: unknown): string | null => {
		if (typeof error === "string") return error;
		if (typeof error !== "object" || error == null) return null;
		if ("message" in error && typeof error.message === "string") {
			return error.message;
		}
		return null;
	};

	const removeProject = useMutation({
		mutationFn: async ({
			project,
			removeDir,
		}: {
			project: Project;
			removeDir: boolean;
		}) => {
			await commands.environmentRemoveProjectByPath(project.path, removeDir);
			return { removeDir };
		},
		onSuccess: async () => {
			setLastDirectoryRemoveFailed(false);
			setLastErrorMessage(null);
			dialog.close(true);
			toastSuccess(tt("projects:toast:project removed"));
			await queryClient.invalidateQueries({
				queryKey: ["environmentProjects"],
			});
			if (
				location.pathname === "/projects/manage" &&
				location.search.projectPath === project.path
			) {
				router.history.back();
			}
		},
		onError: (e, variables) => {
			console.error(e);
			const message = getErrorMessage(e);
			setLastErrorMessage(message);

			if (variables.removeDir) {
				setLastDirectoryRemoveFailed(true);
			} else {
				setLastDirectoryRemoveFailed(false);
				toastThrownError(e);
			}
		},
	});

	return (
		<div className={"contents whitespace-normal"}>
			<DialogTitle>{tc("projects:remove project")}</DialogTitle>
			<div>
				{removeProject.isPending ? (
					<p className={"font-normal"}>{tc("projects:dialog:removing...")}</p>
				) : lastDirectoryRemoveFailed ? (
					<div className="flex flex-col gap-1">
						<p className="font-normal text-destructive">
							{tc("projects:dialog:remove directory failed by lock")}
						</p>
						{lastErrorMessage ? (
							<p className="font-normal text-sm opacity-70 break-all">
								{lastErrorMessage}
							</p>
						) : null}
					</div>
				) : (
					<p className={"font-normal"}>
						{tc("projects:dialog:warn removing project", {
							name: nameFromPath(project.path),
						})}
					</p>
				)}
			</div>
			<DialogFooter className={"flex gap-2"}>
				<Button
					onClick={() => dialog.close(false)}
					disabled={removeProject.isPending}
				>
					{tc("general:button:cancel")}
				</Button>
				<Button
					onClick={() => removeProject.mutate({ project, removeDir: false })}
					className="px-2"
					disabled={removeProject.isPending}
				>
					{tc("projects:button:remove from list")}
				</Button>
				<Button
					onClick={() => removeProject.mutate({ project, removeDir: true })}
					variant={"destructive"}
					className="px-2"
					disabled={!project.is_exists || removeProject.isPending}
				>
					{tc("projects:button:remove directory")}
				</Button>
			</DialogFooter>
		</div>
	);
}
