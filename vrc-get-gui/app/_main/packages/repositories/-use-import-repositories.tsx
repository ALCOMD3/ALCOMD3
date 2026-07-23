import { queryOptions } from "@tanstack/react-query";
import type React from "react";
import { useEffect, useRef, useState } from "react";
import {
	Accordion,
	AccordionContent,
	AccordionItem,
	AccordionTrigger,
} from "@/components/ui/accordion";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { ScrollArea } from "@/components/ui/scroll-area";
import { assertNever } from "@/lib/assert-never";
import type {
	TauriDownloadRepository,
	TauriRepositoryDescriptor,
} from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { callAsyncCommand } from "@/lib/call-async-command";
import { type DialogContext, showDialog } from "@/lib/dialog";
import { tc, tt } from "@/lib/i18n";
import { queryClient } from "@/lib/query-client";
import { toastSuccess } from "@/lib/toast";
import { useEffectEvent } from "@/lib/use-effect-event";
import { RepositoryPackageList } from "./-repository-package-list";

type ParsedRepositories = {
	repositories: TauriRepositoryDescriptor[];
	unparsable_lines: string[];
};

const environmentRepositoriesInfo = queryOptions({
	queryKey: ["environmentRepositoriesInfo"],
	queryFn: commands.environmentRepositoriesInfo,
});

const environmentPackages = queryOptions({
	queryKey: ["environmentPackages"],
	queryFn: commands.environmentPackages,
});

const environmentRepositoryPackageLists = queryOptions({
	queryKey: ["environmentRepositoryPackageLists"],
	queryFn: commands.environmentRepositoryPackageLists,
});

export async function importRepositories() {
	using dialog = showDialog(null, "large-dialog-content overflow-hidden");

	const pickResult = await commands.environmentImportRepositoryPick();
	switch (pickResult.type) {
		case "NoFilePicked":
			// no-op
			return;
		case "ParsedRepositories":
			// continue
			break;
		default:
			assertNever(pickResult, "pickResult");
	}
	console.log("confirmingRepositories", pickResult);

	const repositories = await dialog.ask(ConfirmingRepositoryList, {
		pickResult,
	});
	if (repositories == null) return;

	const packages = await dialog.ask(LoadingRepositories, {
		repositories,
	});
	if (packages == null) return;

	const repositoriesToAdd = await dialog.ask(ConfirmingPackages, {
		packages,
	});
	if (repositoriesToAdd == null) return;

	dialog.setEscapeBehavior(false);
	dialog.replace(<AddingRepositories />);
	await commands.environmentImportAddRepositories(repositoriesToAdd);
	await commands.environmentRefetchPackages();
	toastSuccess(tt("vpm repositories:toast:repository added"));
	dialog.close();

	await Promise.all([
		queryClient.invalidateQueries(environmentRepositoriesInfo),
		queryClient.invalidateQueries(environmentPackages),
		queryClient.invalidateQueries(environmentRepositoryPackageLists),
	]);
}

function shortRepositoryDescription(
	repo: TauriRepositoryDescriptor,
): React.ReactNode {
	if (Object.keys(repo.headers).length > 0) {
		return tc("vpm repositories:dialog:repository with headers", {
			repoUrl: repo.url,
		});
	}
	return repo.url;
}

function ConfirmingRepositoryList({
	pickResult,
	dialog,
}: {
	pickResult: ParsedRepositories;
	dialog: DialogContext<TauriRepositoryDescriptor[] | null>;
}) {
	return (
		<>
			<ScrollArea
				type="scroll"
				className="max-h-[min(560px,calc(100dvh-12rem))] w-full font-normal"
				scrollBarClassName="bg-transparent py-2.5"
			>
				<div className="pr-4">
					<p className={"font-normal whitespace-normal"}>
						{tc("vpm repositories:dialog:confirm repository list")}
					</p>

					<ul className={"list-disc pl-6"}>
						{pickResult.repositories.map((info) => (
							<li key={info.url}>{shortRepositoryDescription(info)}</li>
						))}
					</ul>

					{pickResult.unparsable_lines.length > 0 && (
						<>
							<p className={"font-normal whitespace-normal"}>
								{tc("vpm repositories:dialog:unparsable lines list")}
							</p>
							<ul className={"list-disc pl-6"}>
								{pickResult.unparsable_lines.map((line, idx) => (
									// biome-ignore lint/suspicious/noArrayIndexKey: unchanged
									<li key={idx} className={"whitespace-pre"}>
										{line}
									</li>
								))}
							</ul>
						</>
					)}
				</div>
			</ScrollArea>
			<DialogFooter className={"gap-2"}>
				<Button onClick={() => dialog.close(null)}>
					{tc("general:button:cancel")}
				</Button>
				<Button onClick={() => dialog.close(pickResult.repositories)}>
					{tc("vpm repositories:dialog:button:continue importing repositories")}
				</Button>
			</DialogFooter>
		</>
	);
}

function LoadingRepositories({
	repositories,
	dialog,
}: {
	repositories: TauriRepositoryDescriptor[];
	dialog: DialogContext<
		[TauriRepositoryDescriptor, TauriDownloadRepository][] | null
	>;
}) {
	const cancelRef = useRef<() => void>(() => {});
	const totalCount = repositories.length;
	const [downloaded, setDownloaded] = useState(0);

	const event = useEffectEvent(() => {
		const [cancel, resultPromise] = callAsyncCommand(
			commands.environmentImportDownloadRepositories,
			[repositories],
			(downloaded) => setDownloaded(downloaded),
		);
		cancelRef.current = cancel;
		resultPromise.then(
			(x) => dialog.close(x === "cancelled" ? null : x),
			(error) => dialog.error(error),
		);
	});

	useEffect(() => event(), []);

	return (
		<>
			<div>
				<p>{tc("vpm repositories:dialog:downloading repositories...")}</p>
				<Progress value={downloaded} max={totalCount} />
				<div className={"text-center"}>
					{tc("vpm repositories:dialog:downloaded n/m", {
						downloaded,
						totalCount,
					})}
				</div>
			</div>
			<DialogFooter>
				<Button onClick={() => cancelRef.current?.()}>
					{tc("general:button:cancel")}
				</Button>
			</DialogFooter>
		</>
	);
}

function ConfirmingPackages({
	packages,
	dialog,
}: {
	packages: [TauriRepositoryDescriptor, TauriDownloadRepository][];
	dialog: DialogContext<TauriRepositoryDescriptor[] | null>;
}) {
	async function add() {
		dialog.close(
			packages
				.filter(([_, download]) => download.type === "Success")
				.map(([repo, _]) => repo),
		);
	}

	return (
		<>
			<div className={"flex min-h-0 flex-col font-normal"}>
				<p className={"whitespace-normal"}>
					{tc("vpm repositories:dialog:confirm packages list")}
				</p>
				<ScrollArea
					type="scroll"
					className="h-[min(560px,calc(100dvh-14rem))] w-full"
					scrollBarClassName="bg-transparent py-2.5"
				>
					<div className="pr-4">
						<Accordion type="single" collapsible className="w-full">
							{packages.map(([repo, download]) => {
								let toneClass = "";
								let content: React.ReactNode;
								switch (download.type) {
									case "BadUrl":
										throw new Error("BadUrl should not be here");
									case "Duplicated":
										toneClass = "text-warning";
										content = tc(
											"vpm repositories:dialog:download error:duplicated",
										);
										break;
									case "DownloadError":
										toneClass = "text-destructive";
										content = tc(
											"vpm repositories:dialog:download error:download error",
										);
										break;
									case "Success":
										content = (
											<RepositoryPackageList
												packages={download.value.packages}
											/>
										);
										break;
									default:
										assertNever(download, "download");
								}
								return (
									<AccordionItem value={repo.url} key={repo.url}>
										<AccordionTrigger className={`${toneClass} py-2 text-base`}>
											{shortRepositoryDescription(repo)}
										</AccordionTrigger>
										<AccordionContent className={toneClass}>
											{content}
										</AccordionContent>
									</AccordionItem>
								);
							})}
						</Accordion>
					</div>
				</ScrollArea>
			</div>
			<DialogFooter>
				<Button onClick={() => dialog.close(null)}>
					{tc("general:button:cancel")}
				</Button>
				<Button onClick={add} className={"ml-2"}>
					{tc("vpm repositories:button:add repositories")}
				</Button>
			</DialogFooter>
		</>
	);
}

function AddingRepositories() {
	return (
		<div>
			<p>{tc("vpm repositories:dialog:adding repositories...")}</p>
		</div>
	);
}
