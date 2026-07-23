import React, {
	type ReactNode,
	useEffect,
	useRef,
	useState,
	useSyncExternalStore,
} from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { assertNever } from "@/lib/assert-never";
import { cn } from "@/lib/utils";

export interface DialogContext<in R> {
	close: (arg: R) => void;
	error: (arg: unknown) => void;
	minimize: () => void;
	closing: boolean;
}

type DialogProps<R> = {
	dialog: DialogContext<R>;
};

type DialogResult<P> = P extends DialogProps<infer R> ? R : unknown;

export interface DialogApi {
	replace(state: React.ReactElement): void;
	setEscapeBehavior(escClosableOrBehavior: boolean | EscBehavior): void;

	ask<P extends DialogProps<never>>(
		component: React.JSXElementConstructor<P>,
		props: NoInfer<Omit<P, "dialog">>,
	): Promise<DialogResult<P>>;
	askClosing<P extends DialogProps<never>>(
		component: React.JSXElementConstructor<P>,
		props: NoInfer<Omit<P, "dialog">>,
	): Promise<DialogResult<P>>;
	close(): void;
	[Symbol.dispose](): void;
}

export type EscBehavior = "close" | "none" | "minimize";

type DialogState =
	| {
			type: "before";
	  }
	| {
			type: "asking";
			element: React.ReactElement;
	  }
	| {
			type: "asked";
	  }
	| {
			type: "content";
			element: React.ReactElement;
	  };

export function showDialog(
	initialContent: React.ReactElement | null = null,
	dialogContentClassName?: string,
	escClosableOrBehavior: boolean | EscBehavior = true,
	restoreLabel?: ReactNode,
): DialogApi {
	if (dialogGlobalState == null) throw new Error("No Root is mounted");
	const globalState = dialogGlobalState;
	let escBehavior: EscBehavior =
		escClosableOrBehavior === true
			? "close"
			: escClosableOrBehavior === false
				? "none"
				: escClosableOrBehavior;

	const key = globalState.getKey();
	const dialogState = new SyncStore<DialogState>(
		initialContent == null
			? { type: "before" }
			: { type: "content", element: initialContent },
	);
	let dialogOpened = false;
	let onImplicitClose = closeImpl;

	function closeImpl() {
		if (dialogOpened) globalState.closeDialog(key);
	}

	function askImpl<P extends DialogProps<never>>(
		component: React.JSXElementConstructor<P>,
		props: NoInfer<Omit<P, "dialog">>,
		closing: boolean,
	): Promise<DialogResult<P>> {
		if (dialogState.value.type === "asking")
			throw new Error("another ask in progress");

		let resolve: (result: DialogResult<P>) => void;
		let reject: (error: unknown) => void;
		const promise = new Promise<DialogResult<P>>((r, j) => {
			resolve = r;
			reject = j;
		});

		const dialog: DialogContext<DialogResult<P>> = {
			closing: closing,
			close(r) {
				// A non-closing ask can be followed by another dialog state. Detach its
				// completed content so it cannot intercept input intended for a nested
				// progress dialog while the caller prepares that next state.
				if (closing) closeImpl();
				else dialogState.value = { type: "asked" };
				resolve(r);
			},
			error(e) {
				if (closing) closeImpl();
				else dialogState.value = { type: "asked" };
				reject(e);
			},
			minimize() {
				if (escBehavior === "minimize") {
					globalState.minimizeDialog(key);
				}
			},
		};

		const element = React.createElement<P>(component, {
			...props,
			dialog,
		} as unknown as P);
		onImplicitClose = () => {
			if (dialogState.value.type === "asking") {
				if (!closing) dialogState.value = { type: "asked" };
				resolve(undefined as DialogResult<P>);
			}
			closeImpl();
		};

		dialogState.value = { type: "asking", element };
		mayOpenDialog();

		return promise;
	}

	const result: DialogApi = {
		replace(element) {
			if (dialogState.value.type === "asking")
				throw new Error("another ask in progress");
			dialogState.value = { type: "content", element };
			mayOpenDialog();
		},
		setEscapeBehavior(escClosableOrBehavior) {
			escBehavior =
				escClosableOrBehavior === true
					? "close"
					: escClosableOrBehavior === false
						? "none"
						: escClosableOrBehavior;
			if (dialogOpened) globalState.setEscapeBehavior(key, escBehavior);
		},
		ask<P extends DialogProps<never>>(
			component: React.JSXElementConstructor<P>,
			props: NoInfer<Omit<P, "dialog">>,
		): Promise<DialogResult<P>> {
			return askImpl(component, props, false);
		},
		askClosing<P extends DialogProps<never>>(
			component: React.JSXElementConstructor<P>,
			props: NoInfer<Omit<P, "dialog">>,
		): Promise<DialogResult<P>> {
			return askImpl(component, props, true);
		},
		close() {
			closeImpl();
		},
		[Symbol.dispose]: closeImpl,
	};

	function mayOpenDialog() {
		if (!dialogOpened)
			globalState.openDialog(
				key,
				<DialogBodyElement
					dialogState={dialogState}
					dialogContentClassName={dialogContentClassName}
				/>,
				escBehavior,
				() => onImplicitClose(),
				restoreLabel,
			);
		dialogOpened = true;
	}
	if (dialogState.value.type !== "before") mayOpenDialog();

	return result;
}

function DialogBodyElement({
	dialogState,
	dialogContentClassName,
}: {
	dialogState: SyncStore<DialogState>;
	dialogContentClassName?: string;
}) {
	const state = dialogState.useValue();
	const className = cn(
		"max-h-[calc(100dvh-(var(--spacing)*8))] overflow-y-auto",
		dialogContentClassName,
	);
	switch (state.type) {
		case "before":
			return null;
		case "asking":
			return (
				<DialogContent className={className}>{state.element}</DialogContent>
			);
		case "asked":
			return null;
		case "content":
			return (
				<DialogContent className={className}>{state.element}</DialogContent>
			);
		default:
			assertNever(state);
	}
}

export function openSingleDialog<P extends DialogProps<never>>(
	component: React.JSXElementConstructor<P>,
	props: NoInfer<Omit<P, "dialog">>,
	dialogContentClassName?: string,
	escClosableOrBehavior: boolean | EscBehavior = true,
	restoreLabel?: ReactNode,
): Promise<DialogResult<P>> {
	return showDialog(
		null,
		dialogContentClassName,
		escClosableOrBehavior,
		restoreLabel,
	).askClosing(component, props);
}

interface GlobalState {
	getKey(): number;
	openDialog(
		key: number,
		element: React.ReactElement,
		escBehavior: EscBehavior,
		onImplicitClose: () => void,
		restoreLabel?: ReactNode,
	): void;
	closeDialog(key: number): void;
	minimizeDialog(key: number): void;
	restoreDialog(key: number): void;
	setEscapeBehavior(key: number, escBehavior: EscBehavior): void;
}

let dialogGlobalState: GlobalState | null = null;

const closeDelayMs = 2000;

export function DialogRoot() {
	const keyRef = useRef(0);

	interface ElementState {
		key: number;
		closing: boolean;
		escBehavior: EscBehavior;
		onImplicitClose: () => void;
		minimized: boolean;
		restoreLabel?: ReactNode;
		element: React.ReactElement;
	}
	const [state, setState] = useState<ElementState[]>([]);

	useEffect(() => {
		if (dialogGlobalState != null)
			throw new Error("Multiple DialogRoot is mounted");
		dialogGlobalState = {
			getKey(): number {
				return keyRef.current++;
			},
			openDialog(
				key: number,
				element: React.ReactElement,
				escBehavior: EscBehavior,
				onImplicitClose: () => void,
				restoreLabel?: ReactNode,
			) {
				setState((ary) => [
					...ary,
					{
						key,
						element,
						closing: false,
						escBehavior,
						onImplicitClose,
						minimized: false,
						restoreLabel,
					},
				]);
			},
			closeDialog(key: number) {
				if (closeDelayMs < 0) {
					setState((ary) => ary.filter((x) => x.key !== key));
				} else {
					setState((ary) =>
						ary.map((x) => (x.key !== key ? x : { ...x, closing: true })),
					);
					setTimeout(() => {
						setState((ary) => ary.filter((x) => x.key !== key));
					}, closeDelayMs);
				}
			},
			minimizeDialog(key: number) {
				setState((ary) =>
					ary.map((x) => (x.key !== key ? x : { ...x, minimized: true })),
				);
			},
			restoreDialog(key: number) {
				setState((ary) =>
					ary.map((x) => (x.key !== key ? x : { ...x, minimized: false })),
				);
			},
			setEscapeBehavior(key: number, escBehavior: EscBehavior) {
				setState((ary) =>
					ary.map((x) => (x.key !== key ? x : { ...x, escBehavior })),
				);
			},
		};

		return () => {
			dialogGlobalState = null;
		};
	}, []);

	return state.map(
		({
			closing,
			key,
			element,
			escBehavior,
			onImplicitClose,
			minimized,
			restoreLabel,
		}) => {
			return (
				<React.Fragment key={key}>
					<Dialog
						open={!closing && !minimized}
						onOpenChange={(open) => {
							if (!open) {
								if (escBehavior === "close") {
									onImplicitClose();
								} else if (escBehavior === "minimize") {
									dialogGlobalState?.minimizeDialog(key);
								}
							}
						}}
					>
						{element}
					</Dialog>
					{!closing && minimized && escBehavior === "minimize" && (
						<Button
							className="fixed bottom-4 right-4 z-50 shadow-2xl"
							onClick={() => dialogGlobalState?.restoreDialog(key)}
						>
							{restoreLabel ?? "Restore"}
						</Button>
					)}
				</React.Fragment>
			);
		},
	);
}

class SyncStore<T> {
	private _value: T;
	private listeners: (() => void)[] = [];

	constructor(value: T) {
		this._value = value;
		this.getSnapshot = this.getSnapshot.bind(this);
		this.subscribe = this.subscribe.bind(this);
	}

	private getSnapshot() {
		return this.value;
	}

	private subscribe(onStoreChange: () => void): () => void {
		this.listeners.push(onStoreChange);
		return () => {
			this.listeners = this.listeners.filter((x) => x !== onStoreChange);
		};
	}

	public useValue() {
		return useSyncExternalStore(this.subscribe, this.getSnapshot);
	}

	public get value() {
		return this._value;
	}

	public set value(v: T) {
		this._value = v;
		for (const f of this.listeners) {
			f();
		}
	}
}
