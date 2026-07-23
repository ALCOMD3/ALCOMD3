import { queryOptions, useQuery } from "@tanstack/react-query";
import type React from "react";
import { useState } from "react";
import {
	ReorderableList,
	useReorderableList,
} from "@/components/ReorderableList";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { assertNever } from "@/lib/assert-never";
import type {
	TauriDefaultRepository,
	TauriDownloadRepository,
	TauriDuplicatedReason,
	TauriRemoteRepositoryInfo,
} from "@/lib/bindings";
import { commands } from "@/lib/bindings";
import { type DialogApi, type DialogContext, showDialog } from "@/lib/dialog";
import { tc, tt } from "@/lib/i18n";
import { queryClient } from "@/lib/query-client";
import { toastError, toastSuccess, toastThrownError } from "@/lib/toast";
import { RepositoryPackageList } from "./-repository-package-list";

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

const environmentDefaultRepositories = queryOptions({
	queryKey: ["environmentDefaultRepositories"],
	queryFn: commands.environmentDefaultRepositories,
});

export async function openAddRepositoryDialog() {
	using dialog = showDialog(null, "large-dialog-content overflow-hidden");
	const repoInfo = await dialog.ask(EnteringRepositoryInfo, {});
	if (repoInfo == null) return;
	await addRepositoryImpl(dialog, repoInfo.url, repoInfo.headers);
}

export async function addRepository(
	url: string,
	headers: Record<string, string> | null | undefined,
) {
	const safeHeaders = headers ?? {};
	using dialog = showDialog(null, "large-dialog-content overflow-hidden");
	if (!(await confirmDeepLinkRepositoryDownload(dialog, url, safeHeaders)))
		return;
	await addRepositoryImpl(dialog, url, safeHeaders);
}
async function confirmDeepLinkRepositoryDownload(
	dialog: DialogApi,
	url: string,
	headers: Record<string, string>,
) {
	return await dialog.ask(ConfirmingDeepLinkRepositoryDownload, {
		url,
		headers,
	});
}

async function addRepositoryImpl(
	dialog: DialogApi,
	url: string,
	headers: Record<string, string>,
) {
	dialog.replace(<LoadingRepository cancel={dialog.close} />);
	let info: TauriDownloadRepository;
	try {
		info = await commands.environmentDownloadRepository(url, headers);
	} catch (e) {
		console.error(e);
		toastThrownError(e);
		return;
	}
	switch (info.type) {
		case "BadUrl":
			toastError(tt("vpm repositories:toast:invalid url"));
			return;
		case "DownloadError":
			toastError(
				tt("vpm repositories:toast:load failed", {
					message: info.message,
				}),
			);
			return;
		case "Duplicated":
			await dialog.askClosing(Duplicated, {
				reason: info.reason,
				duplicatedName: info.duplicated_name,
			});
			return;
		case "Success":
			break;
		default:
			assertNever(info, "info");
	}
	if (
		await dialog.askClosing(Confirming, {
			repo: info.value,
			headers: headers,
		})
	) {
		try {
			const result = await commands.environmentAddRepository(url, headers);
			switch (result) {
				case "BadUrl":
					toastError(tt("vpm repositories:toast:invalid url"));
					return;
				case "Success":
					break;
				default:
					assertNever(result, "result");
			}
			await commands.environmentRefetchPackages();
		} catch (e) {
			console.error(e);
			toastThrownError(e);
			return;
		}
		toastSuccess(tt("vpm repositories:toast:repository added"));
		await Promise.all([
			queryClient.invalidateQueries(environmentRepositoriesInfo),
			queryClient.invalidateQueries(environmentPackages),
			queryClient.invalidateQueries(environmentRepositoryPackageLists),
		]);
	}
}

function EnteringRepositoryInfo({
	dialog,
}: {
	dialog: DialogContext<null | {
		url: string;
		headers: Record<string, string>;
	}>;
}) {
	const [url, setUrl] = useState("");

	const reordableListContext = useReorderableList({
		defaultValue: { name: "", value: "" },
		allowEmpty: false,
		reorderable: false,
	});

	let foundHeaderNameError = false;
	let foundHeaderValueError = false;
	let foundDuplicateHeader = false;

	const headerNameSet = new Set<string>();

	for (const { value, name } of reordableListContext.value) {
		const trimedName = name.trim();
		const trimedValue = value.trim();
		if (trimedName !== "" || trimedValue !== "") {
			// header (field) name is token (RFC 9110 section 5.1)
			//   https://www.rfc-editor.org/rfc/rfc9110.html#name-field-names
			// token is defined in 5.6.2
			//   https://www.rfc-editor.org/rfc/rfc9110.html#name-tokens
			if (!trimedName.match(/^[!#$%&'*+\-.^_`|~0-9a-zA-Z]+$/)) {
				foundHeaderNameError = true;
			}

			if (headerNameSet.has(trimedName)) {
				foundDuplicateHeader = true;
			}
			headerNameSet.add(trimedName);

			// header (field) value is field-value (RFC 9110 section 5.5)
			//  note: empty value is allowed
			// field-value    = *field-content
			// field-content  = field-vchar
			//     [ 1*( SP / HTAB / field-vchar ) field-vchar ]
			// field-vchar    = VCHAR / obs-text
			// obs-text       = %x80-FF
			//   ; field-vchar   = [\x21-\x7E\x80-\xFF]
			//   ; field-content = [\x21-\x7E\x80-\xFF]([\t\x20-\x7E\x80-\xFF]+[\x21-\x7E\x80-\xFF])?
			//   ; field-value   = ([\x21-\x7E\x80-\xFF]([\t\x20-\x7E\x80-\xFF]+[\x21-\x7E\x80-\xFF])?)*
			//   ;               = [\t\x20-\x7E\x80-\xFF]* in trimmed value

			// in vrc-get, non-ascii characters are encoded as utf-8 so any non-ascii characters are fit in [\x80-\xFF]
			if (!trimedValue.match(/^[\t\x20-\x7E\u0080-\uFFFF]*$/)) {
				foundHeaderValueError = true;
			}
		}
	}

	const urlError = url.trim() === "";

	const hasError =
		urlError ||
		foundHeaderNameError ||
		foundHeaderValueError ||
		foundDuplicateHeader;

	const onAddRepository = () => {
		const headers: { [name: string]: string } = {};
		for (const header of reordableListContext.value) {
			if (header.name.trim() === "") continue;
			headers[header.name.trim()] = header.value.trim();
		}
		dialog.close({ url, headers });
	};

	return (
		<>
			<div>
				<p className={"font-normal"}>
					{tc("vpm repositories:dialog:enter repository info")}
				</p>
				<Input
					className={"w-full"}
					type={"vpm repositories:url"}
					value={url}
					onChange={(e) => setUrl(e.target.value)}
					placeholder={"https://vpm.anatawa12.com/vpm.json"}
				/>
				<details>
					<summary className={"font-bold"}>
						{tc("vpm repositories:dialog:headers")}
					</summary>
					<ScrollArea
						type="scroll"
						className="max-h-[min(420px,calc(100dvh-18rem))] w-full"
						scrollBarClassName="bg-transparent py-2.5"
					>
						<table className={"w-full"}>
							<thead>
								<tr>
									<th className={"sticky top-0 z-10 bg-background"}>
										{tc("vpm repositories:dialog:header name")}
									</th>
									<th className={"sticky top-0 z-10 bg-background"}>
										{tc("vpm repositories:dialog:header value")}
									</th>
									<th className={"sticky top-0 z-10 bg-background"} />
								</tr>
							</thead>
							<tbody>
								<ReorderableList
									context={reordableListContext}
									renderItem={(value, id) => (
										<>
											<td>
												<div className={"flex"}>
													<Input
														type={"text"}
														value={value.name}
														className={"grow"}
														onChange={(e) =>
															reordableListContext.update(id, (old) => ({
																...old,
																name: e.target.value,
															}))
														}
													/>
												</div>
											</td>
											<td>
												<div className={"flex"}>
													<Input
														type={"text"}
														value={value.value}
														className={"grow"}
														onChange={(e) =>
															reordableListContext.update(id, (old) => ({
																...old,
																value: e.target.value,
															}))
														}
													/>
												</div>
											</td>
										</>
									)}
								/>
							</tbody>
						</table>
					</ScrollArea>
				</details>
				{foundHeaderNameError && (
					<p className={"text-destructive"}>
						{tc("vpm repositories:hint:invalid header names")}
					</p>
				)}
				{foundHeaderValueError && (
					<p className={"text-destructive"}>
						{tc("vpm repositories:hint:invalid header values")}
					</p>
				)}
				{foundDuplicateHeader && (
					<p className={"text-destructive"}>
						{tc("vpm repositories:hint:duplicate headers")}
					</p>
				)}
			</div>
			<DialogFooter>
				<Button onClick={() => dialog.close(null)}>
					{tc("general:button:cancel")}
				</Button>
				<Button
					onClick={onAddRepository}
					className={"ml-2"}
					disabled={hasError}
				>
					{tc("vpm repositories:button:add repository")}
				</Button>
			</DialogFooter>
		</>
	);
}

function ConfirmingDeepLinkRepositoryDownload({
	url,
	headers,
	dialog,
}: {
	url: string;
	headers: { [key: string]: string };
	dialog: DialogContext<boolean>;
}) {
	return (
		<>
			<ScrollArea
				type="scroll"
				className="max-h-[min(560px,calc(100dvh-12rem))] w-full font-normal"
				scrollBarClassName="bg-transparent py-2.5"
			>
				<div className="pr-4">
					<p className={"font-normal"}>
						{tc("vpm repositories:dialog:confirm deep link download")}
					</p>
					<p className={"font-normal break-all"}>
						{tc("vpm repositories:dialog:url", { url })}
					</p>
					{Object.keys(headers).length > 0 && (
						<>
							<p className={"font-normal"}>
								{tc("vpm repositories:dialog:headers")}
							</p>
							<ul className={"list-disc pl-6"}>
								{Object.entries(headers).map(([key, value]) => (
									<li key={key} className="break-all">
										{key}: {value}
									</li>
								))}
							</ul>
						</>
					)}
				</div>
			</ScrollArea>
			<DialogFooter>
				<Button onClick={() => dialog.close(false)}>
					{tc("general:button:cancel")}
				</Button>
				<Button onClick={() => dialog.close(true)} className={"ml-2"}>
					{tc("vpm repositories:button:add repository")}
				</Button>
			</DialogFooter>
		</>
	);
}

function LoadingRepository({ cancel }: { cancel: () => void }) {
	return (
		<>
			<div>
				<p>{tc("vpm repositories:dialog:downloading...")}</p>
			</div>
			<DialogFooter>
				<Button onClick={cancel}>{tc("general:button:cancel")}</Button>
			</DialogFooter>
		</>
	);
}

function Duplicated({
	reason,
	duplicatedName,
	dialog,
}: {
	reason: TauriDuplicatedReason;
	duplicatedName: string;
	dialog: DialogContext<void>;
}) {
	const defaultRepositories =
		useQuery(environmentDefaultRepositories).data ?? [];
	const defaultRepository = defaultRepositories.find(
		(repository) => repository.id === duplicatedName,
	);
	const duplicatedDisplayName =
		defaultRepository == null
			? duplicatedName
			: defaultRepositoryDisplayName(defaultRepository);
	let message: React.ReactNode;
	switch (reason) {
		case "URLDuplicated":
			message = tc("vpm repositories:dialog:url duplicated", {
				name: duplicatedDisplayName,
			});
			break;
		case "IDDuplicated":
			message = tc("vpm repositories:dialog:id duplicated", {
				name: duplicatedDisplayName,
			});
			break;
	}

	return (
		<>
			<div>
				<p>{tc("vpm repositories:dialog:already added")}</p>
				<p>{message}</p>
			</div>
			<DialogFooter>
				<Button onClick={() => dialog.close()}>
					{tc("general:button:ok")}
				</Button>
			</DialogFooter>
		</>
	);
}

function defaultRepositoryDisplayName(
	repository: TauriDefaultRepository,
): string {
	switch (repository.kind) {
		case "officialDefault":
			return tt("vpm repositories:source:official");
		case "curatedDefault":
			return tt("vpm repositories:source:curated");
		default:
			return repository.id;
	}
}

function Confirming({
	repo,
	headers,
	dialog,
}: {
	repo: TauriRemoteRepositoryInfo;
	headers: { [key: string]: string };
	dialog: DialogContext<boolean>;
}) {
	return (
		<>
			<ScrollArea
				type="scroll"
				className="max-h-[min(560px,calc(100dvh-12rem))] w-full font-normal"
				scrollBarClassName="bg-transparent py-2.5"
			>
				<div className="pr-4">
					<p className={"font-normal"}>
						{tc("vpm repositories:dialog:name", { name: repo.display_name })}
					</p>
					<p className={"font-normal"}>
						{tc("vpm repositories:dialog:url", { url: repo.url })}
					</p>
					{Object.keys(headers).length > 0 && (
						<>
							<p className={"font-normal"}>
								{tc("vpm repositories:dialog:headers")}
							</p>
							<ul className={"list-disc pl-6"}>
								{Object.entries(headers).map(([key, value]) => (
									<li key={key}>
										{key}: {value}
									</li>
								))}
							</ul>
						</>
					)}
					<p className={"font-normal"}>
						{tc("vpm repositories:dialog:packages")}
					</p>
					<RepositoryPackageList packages={repo.packages} />
				</div>
			</ScrollArea>
			<DialogFooter>
				<Button onClick={() => dialog.close(false)}>
					{tc("general:button:cancel")}
				</Button>
				<Button onClick={() => dialog.close(true)} className={"ml-2"}>
					{tc("vpm repositories:button:add repository")}
				</Button>
			</DialogFooter>
		</>
	);
}
