import React, { useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { ExternalLink } from "@/components/ExternalLink";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogFooter,
	DialogTitle,
} from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { assertNever } from "@/lib/assert-never";
import type {
	CheckForUpdateResponse,
	UpdateDownloadProgress,
} from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { callAsyncCommand } from "@/lib/call-async-command";
import { type DialogContext, openSingleDialog } from "@/lib/dialog";
import globalInfo, { getHomepageUrl } from "@/lib/global-info";
import { localizeExternalComponent, tc } from "@/lib/i18n";
import { queryClient } from "@/lib/query-client";
import { shouldInstallAfterDownload } from "@/lib/update-download-policy";

type UpdateInstallProgressState =
	| {
			response: CheckForUpdateResponse;
			progress: {
				state: "downloading";
				total: number;
				downloaded: number;
			};
			minimized: boolean;
	  }
	| {
			response: CheckForUpdateResponse;
			progress: {
				state: "waitingForInstall" | "installing";
			};
			minimized: boolean;
	  };

export const latestUpdateCheckQueryKey = ["latestUpdateCheck"] as const;

const UPDATE_REMIND_LATER_STORAGE_KEY = "alcomd3.updateRemindLater";
const UPDATE_REMIND_LATER_DELAY_MS = 7 * 24 * 60 * 60 * 1000;
const DEFAULT_ESTIMATED_UPDATE_TOTAL_SIZE = 20 * 1000 * 1000;
const HTTPS_URL_REGEX =
	/https:\/\/[a-zA-Z0-9]+(?:\.[a-zA-Z0-9]+)+\/[a-zA-Z0-9$\-_.+!*'()%/?#]*/g;

const ESTIMATED_UPDATE_TOTAL_SIZE_BY_OS: Record<string, number | undefined> = {
	WindowsNT: 6 * 1000 * 1000,
	Linux: 90 * 1000 * 1000,
	Darwin: DEFAULT_ESTIMATED_UPDATE_TOTAL_SIZE,
};

type UpdateReminder = {
	latestVersion: string;
	remindAfter: number;
};

function normalizeUpdateReminder(value: unknown): UpdateReminder | null {
	if (
		typeof value === "object" &&
		value !== null &&
		"latestVersion" in value &&
		"remindAfter" in value &&
		typeof value.latestVersion === "string" &&
		typeof value.remindAfter === "number" &&
		Number.isFinite(value.remindAfter)
	) {
		return {
			latestVersion: value.latestVersion,
			remindAfter: value.remindAfter,
		};
	}

	return null;
}

function parseUpdateReminder(value: string): UpdateReminder | null {
	try {
		return normalizeUpdateReminder(JSON.parse(value));
	} catch {
		// Ignore invalid persisted state; update checks can still show the dialog.
	}

	return null;
}

function readUpdateReminder(): UpdateReminder | null {
	try {
		const value = localStorage.getItem(UPDATE_REMIND_LATER_STORAGE_KEY);
		return value == null ? null : parseUpdateReminder(value);
	} catch {
		return null;
	}
}

async function readPersistedUpdateReminder(): Promise<UpdateReminder | null> {
	try {
		const reminder = normalizeUpdateReminder(
			await commands.environmentUpdateReminder(),
		);
		return reminder ?? readUpdateReminder();
	} catch (e) {
		console.error("failed to read persisted update reminder", e);
		return readUpdateReminder();
	}
}

export async function shouldSkipUpdateDialogFromReminder(
	response: CheckForUpdateResponse,
): Promise<boolean> {
	const reminder = await readPersistedUpdateReminder();
	if (reminder == null) return false;
	if (reminder.latestVersion !== response.latest_version) return false;

	const shouldSkip = reminder.remindAfter > Date.now();
	if (!shouldSkip) {
		try {
			localStorage.removeItem(UPDATE_REMIND_LATER_STORAGE_KEY);
		} catch {
			// Ignore cleanup failure; it should not block the update dialog.
		}
		try {
			await commands.environmentSetUpdateReminder(null);
		} catch (e) {
			console.error("failed to clear persisted update reminder", e);
		}
	}

	return shouldSkip;
}

async function saveUpdateReminder(response: CheckForUpdateResponse) {
	const reminder: UpdateReminder = {
		latestVersion: response.latest_version,
		remindAfter: Date.now() + UPDATE_REMIND_LATER_DELAY_MS,
	};

	try {
		localStorage.setItem(
			UPDATE_REMIND_LATER_STORAGE_KEY,
			JSON.stringify(reminder),
		);
	} catch (e) {
		console.error("failed to save update reminder", e);
	}

	try {
		await commands.environmentSetUpdateReminder(reminder);
	} catch (e) {
		console.error("failed to save persisted update reminder", e);
	}
}

function updateDescriptionForLanguages(
	response: CheckForUpdateResponse,
	languages: readonly string[],
	noDescriptionFallback: string,
) {
	const fallback = response.update_description?.trim();

	for (const language of languages) {
		const localized =
			response.update_description_localizations?.[language]?.trim();
		if (localized) return appendFallbackLinks(localized, fallback);
	}

	if (fallback) return fallback;

	return noDescriptionFallback;
}

function appendFallbackLinks(text: string, fallback: string | undefined) {
	if (!fallback) return text;

	const existingUrls = new Set(urlsInText(text));
	const missingUrls = urlsInText(fallback).filter(
		(url) => !existingUrls.has(url),
	);
	if (missingUrls.length === 0) return text;

	return `${text}\n\n${missingUrls.join("\n")}`;
}

function urlsInText(text: string) {
	return Array.from(text.matchAll(HTTPS_URL_REGEX), (match) => match[0]);
}

let updateInstallProgressState: UpdateInstallProgressState | null = null;
const updateInstallProgressListeners = new Set<() => void>();

function emitUpdateInstallProgress() {
	for (const listener of updateInstallProgressListeners) listener();
}

function setUpdateInstallProgressState(
	state: UpdateInstallProgressState | null,
) {
	updateInstallProgressState = state;
	emitUpdateInstallProgress();
}

export function subscribeUpdateInstallProgress(listener: () => void) {
	updateInstallProgressListeners.add(listener);
	return () => updateInstallProgressListeners.delete(listener);
}

function getUpdateInstallProgressSnapshot() {
	return updateInstallProgressState;
}

export function getUpdateInstallProgressActiveSnapshot() {
	return (
		updateInstallProgressState?.progress.state === "downloading" ||
		updateInstallProgressState?.progress.state === "installing"
	);
}

function minimizeUpdateInstallProgress() {
	if (updateInstallProgressState == null) return;
	setUpdateInstallProgressState({
		...updateInstallProgressState,
		minimized: true,
	});
}

export function restoreUpdateInstallProgress() {
	if (updateInstallProgressState == null) return false;
	setUpdateInstallProgressState({
		...updateInstallProgressState,
		minimized: false,
	});
	return true;
}

function updateInstallProgress(progress: UpdateDownloadProgress) {
	const state = updateInstallProgressState;
	if (state == null) return;

	switch (progress.type) {
		case "DownloadProgress": {
			if (state.progress.state !== "downloading") return;
			if (progress.total != null) {
				setUpdateInstallProgressState({
					...state,
					progress: {
						...state.progress,
						downloaded: state.progress.downloaded + progress.received,
						total: progress.total,
					},
				});
				return;
			}

			const estimatedTotalSize =
				ESTIMATED_UPDATE_TOTAL_SIZE_BY_OS[globalInfo.osType] ??
				DEFAULT_ESTIMATED_UPDATE_TOTAL_SIZE;
			const downloaded = state.progress.downloaded + progress.received;

			setUpdateInstallProgressState({
				...state,
				progress: {
					...state.progress,
					downloaded:
						downloaded > estimatedTotalSize ? estimatedTotalSize : downloaded,
					total: estimatedTotalSize,
				},
			});
			break;
		}
		case "DownloadComplete":
			setUpdateInstallProgressState({
				...state,
				progress: {
					state: "waitingForInstall",
				},
			});
			break;
		default:
			assertNever(progress);
	}
}

function showDownloadedUpdate(
	response: CheckForUpdateResponse,
	minimized: boolean,
) {
	setUpdateInstallProgressState({
		response,
		progress: {
			state: "waitingForInstall",
		},
		minimized,
	});
}

function startUpdateDownload(
	response: CheckForUpdateResponse,
	automatic: boolean,
) {
	if (automatic) {
		if (updateInstallProgressState != null) return;
	} else if (restoreUpdateInstallProgress()) {
		return;
	}

	const [, commandPromise] = callAsyncCommand(
		commands.utilDownloadUpdate,
		[automatic, response.version],
		updateInstallProgress,
	);

	setUpdateInstallProgressState({
		response,
		progress: {
			state: "downloading",
			downloaded: 0,
			total: 100,
		},
		minimized: automatic,
	});

	void commandPromise
		.then((result) => {
			if (result === "cancelled") {
				setUpdateInstallProgressState(null);
				return;
			}

			const downloadedResponse = {
				...response,
				automatic_download: false,
				update_downloaded: true,
			};
			queryClient.setQueryData<CheckForUpdateResponse | null>(
				latestUpdateCheckQueryKey,
				downloadedResponse,
			);
			showDownloadedUpdate(
				downloadedResponse,
				updateInstallProgressState?.minimized ?? automatic,
			);
			if (shouldInstallAfterDownload(automatic)) {
				void installDownloadedUpdate();
			}
		})
		.catch((error) => {
			console.error(error);
			setUpdateInstallProgressState(null);
			if (!automatic) {
				void openSingleDialog(CheckForUpdateFailedDialog, {});
			}
		});
}

export function handleAutomaticUpdate(response: CheckForUpdateResponse) {
	if (response.update_downloaded) {
		showDownloadedUpdate(response, true);
	} else if (response.automatic_download) {
		startUpdateDownload(response, true);
	}
}

async function installDownloadedUpdate() {
	const state = updateInstallProgressState;
	if (state == null || state.progress.state !== "waitingForInstall") return;

	setUpdateInstallProgressState({
		...state,
		progress: { state: "installing" },
	});

	try {
		await commands.utilInstallDownloadedUpdate();
	} catch (error) {
		console.error(error);
		try {
			const response = await refreshUpdateCheck(true);
			if (response?.update_downloaded) {
				showDownloadedUpdate(response, false);
			} else {
				setUpdateInstallProgressState(null);
			}
		} catch {
			setUpdateInstallProgressState(null);
		}
		await openSingleDialog(CheckForUpdateFailedDialog, {});
	}
}

export function UpdateInstallProgressHost() {
	const state = useSyncExternalStore(
		subscribeUpdateInstallProgress,
		getUpdateInstallProgressSnapshot,
	);

	if (state == null) return null;

	return (
		<Dialog
			open={!state.minimized}
			onOpenChange={(open) => {
				if (!open) minimizeUpdateInstallProgress();
			}}
		>
			<DialogContent
				onEscapeKeyDown={(event) => {
					event.preventDefault();
					minimizeUpdateInstallProgress();
				}}
			>
				<DialogTitle>{tc("check update:dialog:title")}</DialogTitle>
				<div>
					{state.progress.state === "downloading" && (
						<>
							<p>{tc("check update:dialog:downloading...")}</p>
							<Progress
								value={state.progress.downloaded}
								max={state.progress.total}
							/>
						</>
					)}
					{state.progress.state === "waitingForInstall" && (
						<p>{tc("check update:dialog:ready to install")}</p>
					)}
					{state.progress.state === "installing" && (
						<p>{tc("check update:dialog:installing...")}</p>
					)}
				</div>
				{state.progress.state === "waitingForInstall" && (
					<DialogFooter className="gap-2">
						<Button onClick={minimizeUpdateInstallProgress}>
							{tc("general:button:close")}
						</Button>
						<Button onClick={installDownloadedUpdate}>
							{tc("check update:dialog:install")}
						</Button>
					</DialogFooter>
				)}
			</DialogContent>
		</Dialog>
	);
}

export async function refreshUpdateCheck(manual: boolean) {
	try {
		const checkVersion = await commands.utilCheckForUpdate(manual);
		queryClient.setQueryData<CheckForUpdateResponse | null>(
			latestUpdateCheckQueryKey,
			checkVersion,
		);
		return checkVersion;
	} catch (e) {
		console.error(e);
		throw e;
	}
}

export async function checkForUpdates() {
	if (restoreUpdateInstallProgress()) return;

	try {
		const checkVersion = await refreshUpdateCheck(true);
		if (checkVersion?.update_downloaded) {
			showDownloadedUpdate(checkVersion, false);
		} else if (checkVersion) {
			await openSingleDialog(CheckForUpdateMessage, {
				response: checkVersion,
			});
		} else {
			await openSingleDialog(CheckForUpdateNoUpdatesDialog, {});
		}
	} catch {
		await openSingleDialog(CheckForUpdateFailedDialog, {});
	}
}

export function CheckForUpdateMessage({
	response,
	dialog,
}: {
	response: CheckForUpdateResponse;
	dialog: DialogContext<boolean>;
}) {
	const startDownload = async () => {
		startUpdateDownload(response, false);
		dialog.close(false);
	};

	const openAlcomWebsite = async () => {
		await commands.utilOpenUrl(getHomepageUrl());
	};

	const remindLater = async () => {
		await saveUpdateReminder(response);
		dialog.close(false);
	};

	let message: React.ReactNode;

	switch (response.updater_status) {
		case "Updatable":
			message = <p>{tc("check update:dialog:new version description")}</p>;
			break;
		case "NoPlatform":
			message = (
				<p>{tc("check update:dialog:new version no platform description")}</p>
			);
			break;
		case "NotUpdatable":
			message = (
				<p>{tc("check update:dialog:new version not updatable description")}</p>
			);
			break;
		case "UpdaterDisabled":
			message = (
				<p>
					{tc(
						"check update:dialog:new version updater disabled base description",
					)}
					<br />
					{localizeExternalComponent(response.updater_disabled_messages, {
						localized:
							"check update:dialog:new version updater how to upgrade fallback",
					})}
				</p>
			);
			break;
		default:
			assertNever(response.updater_status);
	}

	const withDownloadButton = response.updater_status === "Updatable";

	return (
		<>
			<DialogTitle>{tc("check update:dialog:title")}</DialogTitle>
			<div>
				{message}
				<p>
					{tc("check update:dialog:current version")} {response.current_version}
				</p>
				<p>
					{tc("check update:dialog:latest version")} {response.latest_version}
				</p>
				<h3>{tc("check update:dialog:changelog")}</h3>
				<p className={"whitespace-pre-wrap"}>
					<LocalizedUpdateDescription response={response} />
				</p>
			</div>
			<DialogFooter className={"gap-2"}>
				<Button onClick={() => dialog.close(false)}>
					{tc("general:button:close")}
				</Button>
				<Button onClick={remindLater}>
					{tc("check update:dialog:remind later")}
				</Button>
				{withDownloadButton && (
					<Button onClick={startDownload}>
						{tc("check update:dialog:update")}
					</Button>
				)}
				<Button onClick={openAlcomWebsite}>
					{tc("check update:dialog:open download page")}
				</Button>
			</DialogFooter>
		</>
	);
}

function LocalizedUpdateDescription({
	response,
}: {
	response: CheckForUpdateResponse;
}) {
	const { i18n } = useTranslation();
	const updateDescription = updateDescriptionForLanguages(
		response,
		i18n.languages,
		i18n.t("check update:dialog:no changelog description"),
	);

	return <LinkedText text={updateDescription} />;
}

function CheckForUpdateNoUpdatesDialog({
	dialog,
}: {
	dialog: DialogContext<void>;
}) {
	return (
		<>
			<DialogTitle>{tc("check update:dialog:no updates title")}</DialogTitle>
			<p className="whitespace-normal">
				{tc("check update:dialog:no updates description")}
			</p>
			<DialogFooter>
				<Button onClick={() => dialog.close()}>
					{tc("general:button:close")}
				</Button>
			</DialogFooter>
		</>
	);
}

function CheckForUpdateFailedDialog({
	dialog,
}: {
	dialog: DialogContext<void>;
}) {
	return (
		<>
			<DialogTitle>{tc("check update:dialog:failed title")}</DialogTitle>
			<p className="whitespace-normal">
				{tc("check update:dialog:failed description")}
			</p>
			<DialogFooter>
				<Button onClick={() => dialog.close()}>
					{tc("general:button:close")}
				</Button>
			</DialogFooter>
		</>
	);
}

const LinkedText = React.memo(({ text }: { text: string }) => {
	const components: React.ReactNode[] = [];
	let lastMatchEnd = 0;
	for (const match of text.matchAll(HTTPS_URL_REGEX)) {
		const leading = text.substring(lastMatchEnd, match.index);
		components.push(leading);
		components.push(<ExternalLink href={match[0]}>{match[0]}</ExternalLink>);
		lastMatchEnd = match.index + match[0].length;
	}
	components.push(text.substring(lastMatchEnd));

	return React.createElement(React.Fragment, {}, components);
});
