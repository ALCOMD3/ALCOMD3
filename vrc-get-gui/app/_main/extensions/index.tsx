"use client";

import {
	closestCenter,
	DndContext,
	type DragEndEvent,
	KeyboardSensor,
	PointerSensor,
	useSensor,
	useSensors,
} from "@dnd-kit/core";
import {
	arrayMove,
	SortableContext,
	sortableKeyboardCoordinates,
	useSortable,
	verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
	queryOptions,
	useMutation,
	useQuery,
	useQueryClient,
} from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { ArrowUpDown, Blocks, GripVertical } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { HNavBar, HNavBarText, VStack } from "@/components/layout";
import { ScrollPageContainer } from "@/components/ScrollPageContainer";
import { SIDEBAR_EXTENSION_DEFINITIONS } from "@/components/sidebar-extension-definitions";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
	Dialog,
	DialogClose,
	DialogContent,
	DialogFooter,
	DialogHeader,
	DialogTitle,
	DialogTrigger,
} from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import { commands, type SidebarExtension } from "@/lib/bindings";
import { tc, tt } from "@/lib/i18n";
import { toastThrownError } from "@/lib/toast";

export const Route = createFileRoute("/_main/extensions/")({
	component: ExtensionsPage,
});

function ExtensionsPage() {
	return (
		<VStack>
			<HNavBar
				className="shrink-0"
				leading={<HNavBarText>{tc("extensions")}</HNavBarText>}
			/>
			<ScrollPageContainer viewportClassName="rounded-xl shadow-xl h-full">
				<main className="flex w-full flex-col gap-3 p-2 compact:p-1">
					<ExtensionsSortCard />
					<ExtensionsManageCard />
				</main>
			</ScrollPageContainer>
		</VStack>
	);
}

const SIDEBAR_EXTENSIONS_QUERY = queryOptions({
	queryKey: ["environmentGetSidebarExtensions"],
	queryFn: commands.environmentGetSidebarExtensions,
	initialData: [
		{ id: "projects", installed: true, visible: true },
		{ id: "packages", installed: true, visible: true },
		{ id: "settings", installed: true, visible: true },
		{ id: "mcp", installed: true, visible: true },
		{ id: "log", installed: true, visible: true },
	],
});

function extensionLabel(id: string) {
	const definition = SIDEBAR_EXTENSION_DEFINITIONS[id];
	if (!definition) return id;
	return tc(definition.labelKey);
}

function useSidebarExtensions() {
	return useQuery(SIDEBAR_EXTENSIONS_QUERY);
}

function isSortableSidebarExtension(extension: SidebarExtension) {
	return extension.installed && extension.visible;
}

function mergeSidebarExtensionOrder(
	extensions: SidebarExtension[],
	orderedExtensions: SidebarExtension[],
) {
	const orderedIterator = orderedExtensions[Symbol.iterator]();
	return extensions.map((extension) =>
		isSortableSidebarExtension(extension)
			? (orderedIterator.next().value ?? extension)
			: extension,
	);
}

function ExtensionsSortCard() {
	const queryClient = useQueryClient();
	const extensionsQuery = useSidebarExtensions();
	const [sortDialogOpen, setSortDialogOpen] = useState(false);
	const sensors = useSensors(
		useSensor(PointerSensor, {
			activationConstraint: {
				distance: 4,
			},
		}),
		useSensor(KeyboardSensor, {
			coordinateGetter: sortableKeyboardCoordinates,
		}),
	);
	const sortableExtensions = useMemo(
		() => (extensionsQuery.data ?? []).filter(isSortableSidebarExtension),
		[extensionsQuery.data],
	);
	const [orderedExtensions, setOrderedExtensions] =
		useState<SidebarExtension[]>(sortableExtensions);

	useEffect(() => {
		if (!sortDialogOpen) return;
		setOrderedExtensions(sortableExtensions);
	}, [sortDialogOpen, sortableExtensions]);

	const hasOrderChange = useMemo(() => {
		if (sortableExtensions.length !== orderedExtensions.length) return true;
		for (let i = 0; i < orderedExtensions.length; i++) {
			if (sortableExtensions[i]?.id !== orderedExtensions[i]?.id) {
				return true;
			}
		}
		return false;
	}, [orderedExtensions, sortableExtensions]);

	const reorderSidebarExtensions = useMutation({
		mutationFn: async (next: SidebarExtension[]) => {
			await commands.environmentSetSidebarExtensionOrder(next);
		},
		onMutate: async (next) => {
			await queryClient.cancelQueries({
				queryKey: SIDEBAR_EXTENSIONS_QUERY.queryKey,
			});
			const previous = queryClient.getQueryData<SidebarExtension[]>(
				SIDEBAR_EXTENSIONS_QUERY.queryKey,
			);
			queryClient.setQueryData(SIDEBAR_EXTENSIONS_QUERY.queryKey, next);
			return { previous };
		},
		onError: (error, _next, context) => {
			toastThrownError(error);
			if (context?.previous) {
				queryClient.setQueryData(
					SIDEBAR_EXTENSIONS_QUERY.queryKey,
					context.previous,
				);
			}
		},
		onSuccess: () => {
			setSortDialogOpen(false);
		},
		onSettled: () => {
			queryClient.invalidateQueries({
				queryKey: SIDEBAR_EXTENSIONS_QUERY.queryKey,
			});
		},
	});

	const handleDragEnd = ({ active, over }: DragEndEvent) => {
		if (!over || active.id === over.id) return;
		setOrderedExtensions((current) => {
			const oldIndex = current.findIndex(
				(extension) => extension.id === active.id,
			);
			const newIndex = current.findIndex(
				(extension) => extension.id === over.id,
			);
			if (oldIndex < 0 || newIndex < 0) return current;
			return arrayMove(current, oldIndex, newIndex);
		});
	};

	return (
		<Card className="p-4 compact:p-3">
			<h2 className="mb-2 text-lg">{tc("extensions")}</h2>
			<p className="text-sm whitespace-normal">
				{tc("extensions:description")}
			</p>
			<div className="mt-3">
				<Dialog open={sortDialogOpen} onOpenChange={setSortDialogOpen}>
					<DialogTrigger asChild>
						<Button className={"compact:h-10"}>
							<ArrowUpDown className="mr-2 size-4" />
							{tc("extensions:button:sort sidebar")}
						</Button>
					</DialogTrigger>
					<DialogContent className={"max-w-[600px]"}>
						<DialogHeader>
							<DialogTitle>{tc("extensions:dialog:sort sidebar")}</DialogTitle>
						</DialogHeader>
						<p className="text-sm whitespace-normal">
							{tc("extensions:dialog:sort sidebar description")}
						</p>
						<DndContext
							sensors={sensors}
							collisionDetection={closestCenter}
							onDragEnd={handleDragEnd}
						>
							<SortableContext
								items={orderedExtensions.map((extension) => extension.id)}
								strategy={verticalListSortingStrategy}
							>
								<div className="mt-3 flex flex-col gap-2">
									{orderedExtensions.map((extension) => (
										<SortableExtensionItem
											key={extension.id}
											extension={extension}
											disabled={reorderSidebarExtensions.isPending}
										/>
									))}
								</div>
							</SortableContext>
						</DndContext>
						<DialogFooter>
							<DialogClose asChild>
								<Button>{tc("general:button:cancel")}</Button>
							</DialogClose>
							<Button
								onClick={() => {
									reorderSidebarExtensions.mutate(
										mergeSidebarExtensionOrder(
											extensionsQuery.data ?? [],
											orderedExtensions,
										),
									);
								}}
								disabled={reorderSidebarExtensions.isPending || !hasOrderChange}
							>
								{tc("extensions:button:save")}
							</Button>
						</DialogFooter>
					</DialogContent>
				</Dialog>
			</div>
		</Card>
	);
}

function SortableExtensionItem({
	extension,
	disabled,
}: {
	extension: SidebarExtension;
	disabled: boolean;
}) {
	const {
		attributes,
		listeners,
		setNodeRef,
		setActivatorNodeRef,
		transform,
		transition,
		isDragging,
	} = useSortable({
		id: extension.id,
		disabled,
	});
	const Icon = SIDEBAR_EXTENSION_DEFINITIONS[extension.id]?.icon ?? Blocks;

	return (
		<div
			ref={setNodeRef}
			style={{
				transform: CSS.Transform.toString(transform),
				transition,
			}}
			className={`flex items-center justify-between gap-3 rounded-md border border-border bg-secondary/30 px-3 py-2 ${
				isDragging ? "z-10 opacity-70 shadow-lg" : ""
			}`}
		>
			<div className="flex min-w-0 items-center gap-3">
				<Icon className="size-5 shrink-0 text-primary" />
				<p className="truncate font-normal">{extensionLabel(extension.id)}</p>
			</div>
			<Button
				ref={setActivatorNodeRef}
				variant={"ghost"}
				size={"icon"}
				disabled={disabled}
				aria-label={tt("extensions:button:drag to reorder")}
				className="cursor-grab touch-none active:cursor-grabbing"
				{...attributes}
				{...listeners}
			>
				<GripVertical className="size-5" />
			</Button>
		</div>
	);
}

function ExtensionsManageCard() {
	const queryClient = useQueryClient();
	const extensionsQuery = useSidebarExtensions();

	const setInstalled = useMutation({
		mutationFn: async ({ id, installed }: { id: string; installed: boolean }) =>
			await commands.environmentSetSidebarExtensionInstalled(id, installed),
		onMutate: async ({ id, installed }) => {
			await queryClient.cancelQueries({
				queryKey: SIDEBAR_EXTENSIONS_QUERY.queryKey,
			});
			const previous = queryClient.getQueryData<SidebarExtension[]>(
				SIDEBAR_EXTENSIONS_QUERY.queryKey,
			);
			queryClient.setQueryData(SIDEBAR_EXTENSIONS_QUERY.queryKey, (current) => {
				if (!current) return current;
				return current.map((extension) =>
					extension.id === id
						? {
								...extension,
								installed,
								visible: installed ? extension.visible : false,
							}
						: extension,
				);
			});
			return { previous };
		},
		onError: (error, _args, context) => {
			toastThrownError(error);
			if (context?.previous) {
				queryClient.setQueryData(
					SIDEBAR_EXTENSIONS_QUERY.queryKey,
					context.previous,
				);
			}
		},
		onSettled: () => {
			queryClient.invalidateQueries({
				queryKey: SIDEBAR_EXTENSIONS_QUERY.queryKey,
			});
		},
	});

	const setVisible = useMutation({
		mutationFn: async ({ id, visible }: { id: string; visible: boolean }) =>
			await commands.environmentSetSidebarExtensionVisible(id, visible),
		onMutate: async ({ id, visible }) => {
			await queryClient.cancelQueries({
				queryKey: SIDEBAR_EXTENSIONS_QUERY.queryKey,
			});
			const previous = queryClient.getQueryData<SidebarExtension[]>(
				SIDEBAR_EXTENSIONS_QUERY.queryKey,
			);
			queryClient.setQueryData(SIDEBAR_EXTENSIONS_QUERY.queryKey, (current) => {
				if (!current) return current;
				return current.map((extension) =>
					extension.id === id ? { ...extension, visible } : extension,
				);
			});
			return { previous };
		},
		onError: (error, _args, context) => {
			toastThrownError(error);
			if (context?.previous) {
				queryClient.setQueryData(
					SIDEBAR_EXTENSIONS_QUERY.queryKey,
					context.previous,
				);
			}
		},
		onSettled: () => {
			queryClient.invalidateQueries({
				queryKey: SIDEBAR_EXTENSIONS_QUERY.queryKey,
			});
		},
	});

	const extensions = (extensionsQuery.data ?? []).filter(
		(extension) => SIDEBAR_EXTENSION_DEFINITIONS[extension.id]?.manageable,
	);
	const installedExtensions = extensions.filter(
		(extension) => extension.installed,
	);
	const uninstalledExtensions = extensions.filter(
		(extension) => !extension.installed,
	);
	const isBusy = setInstalled.isPending || setVisible.isPending;

	return (
		<Card className="p-4 compact:p-3">
			<h2 className="mb-2 text-lg">{tc("extensions:manage")}</h2>
			<p className="text-sm whitespace-normal">
				{tc("extensions:manage description")}
			</p>
			<div className="mt-5 flex flex-col gap-6">
				<ExtensionSection
					title={tc("extensions:installed")}
					emptyText={tc("extensions:empty installed")}
					extensions={installedExtensions}
					renderExtension={(extension) => (
						<ExtensionManageItem
							key={extension.id}
							extension={extension}
							action={
								<Button
									variant={"ghost-destructive"}
									className="border border-destructive/60"
									disabled={isBusy}
									onClick={() =>
										setInstalled.mutate({
											id: extension.id,
											installed: false,
										})
									}
								>
									{tc("extensions:button:uninstall")}
								</Button>
							}
							trailing={
								<div className="flex items-center gap-2">
									<span className="text-sm text-muted-foreground">
										{tc("extensions:show in sidebar")}
									</span>
									<Switch
										checked={extension.visible}
										disabled={isBusy}
										aria-label={tt("extensions:show in sidebar")}
										onCheckedChange={(visible) =>
											setVisible.mutate({
												id: extension.id,
												visible,
											})
										}
									/>
								</div>
							}
						/>
					)}
				/>
				<ExtensionSection
					title={tc("extensions:not installed")}
					emptyText={tc("extensions:empty not installed")}
					extensions={uninstalledExtensions}
					renderExtension={(extension) => (
						<ExtensionManageItem
							key={extension.id}
							extension={extension}
							action={
								<Button
									className="border border-primary bg-transparent text-primary hover:bg-primary/10"
									disabled={isBusy}
									onClick={() =>
										setInstalled.mutate({
											id: extension.id,
											installed: true,
										})
									}
								>
									{tc("extensions:button:install")}
								</Button>
							}
						/>
					)}
				/>
			</div>
		</Card>
	);
}

function ExtensionSection({
	title,
	emptyText,
	extensions,
	renderExtension,
}: {
	title: ReactNode;
	emptyText: ReactNode;
	extensions: SidebarExtension[];
	renderExtension: (extension: SidebarExtension) => ReactNode;
}) {
	return (
		<section>
			<div className="mb-3 flex items-center gap-2">
				<h3 className="text-base font-medium">{title}</h3>
				<span className="rounded-full bg-secondary px-2 py-0.5 text-xs text-secondary-foreground">
					{extensions.length}
				</span>
			</div>
			{extensions.length > 0 ? (
				<div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
					{extensions.map(renderExtension)}
				</div>
			) : (
				<div className="rounded-lg border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground">
					{emptyText}
				</div>
			)}
		</section>
	);
}

function ExtensionManageItem({
	extension,
	action,
	trailing,
}: {
	extension: SidebarExtension;
	action: ReactNode;
	trailing?: ReactNode;
}) {
	const Icon = SIDEBAR_EXTENSION_DEFINITIONS[extension.id]?.icon ?? Blocks;

	return (
		<Card className="flex min-h-40 flex-col gap-5 bg-secondary/25 p-4 shadow-sm">
			<div className="flex min-w-0 items-start gap-3">
				<div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary">
					<Icon className="size-5" />
				</div>
				<div className="min-w-0">
					<h4 className="truncate font-medium">
						{extensionLabel(extension.id)}
					</h4>
					<p className="mt-1 truncate text-sm text-muted-foreground">
						{extension.id}
					</p>
				</div>
			</div>
			<div className="mt-auto flex flex-wrap items-center justify-between gap-3">
				{action}
				{trailing}
			</div>
		</Card>
	);
}
