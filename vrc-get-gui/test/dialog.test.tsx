import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	type DialogApi,
	type DialogContext,
	DialogRoot,
	showDialog,
} from "@/lib/dialog";

function Prompt({
	dialog,
	value,
}: {
	dialog: DialogContext<string>;
	value: string;
}) {
	return (
		<>
			<DialogTitle>Prompt</DialogTitle>
			<DialogDescription>Choose a value.</DialogDescription>
			<button type="button" onClick={() => dialog.close(value)}>
				Choose {value}
			</button>
		</>
	);
}

function TestHost({ onProgressEscape }: { onProgressEscape?: () => void }) {
	return (
		<>
			{onProgressEscape && (
				<Dialog open>
					<DialogContent
						onEscapeKeyDown={(event) => {
							event.preventDefault();
							onProgressEscape();
						}}
					>
						<DialogTitle>Progress</DialogTitle>
						<DialogDescription>Work is in progress.</DialogDescription>
					</DialogContent>
				</Dialog>
			)}
			<DialogRoot />
		</>
	);
}

async function openPrompt(dialog: DialogApi, value: string) {
	let result: Promise<string> | undefined;
	await act(async () => {
		result = dialog.ask(Prompt, { value });
	});
	if (result == null) throw new Error("Prompt did not open");
	return { result };
}

describe("DialogRoot", () => {
	let root: Root;
	let host: HTMLDivElement;

	beforeEach(async () => {
		Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
		host = document.createElement("div");
		document.body.append(host);
		root = createRoot(host);
		await act(async () => root.render(<TestHost />));
	});

	afterEach(async () => {
		await act(async () => root.unmount());
		Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: false });
	});

	it("detaches a completed non-closing prompt before the next dialog state", async () => {
		const dialog = showDialog();
		const { result: firstResult } = await openPrompt(dialog, "first");
		const firstButton = document.querySelector("button");
		expect(firstButton?.textContent).toContain("Choose first");

		await act(async () => firstButton?.click());
		await expect(firstResult).resolves.toBe("first");
		expect(document.querySelector("button")).toBeNull();

		const onProgressEscape = vi.fn();
		await act(async () =>
			root.render(<TestHost onProgressEscape={onProgressEscape} />),
		);
		await act(async () => {
			document.dispatchEvent(
				new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
			);
		});
		expect(onProgressEscape).toHaveBeenCalledOnce();
		await act(async () => root.render(<TestHost />));

		const { result: secondResult } = await openPrompt(dialog, "second");
		const secondButton = document.querySelector("button");
		expect(secondButton?.textContent).toContain("Choose second");

		await act(async () => secondButton?.click());
		await expect(secondResult).resolves.toBe("second");

		await act(async () => {
			dialog.close();
		});
	});
});
