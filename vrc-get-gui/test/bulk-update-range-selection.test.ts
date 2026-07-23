import { describe, expect, test } from "vitest";
import {
	applyPackageRangeSelection,
	collectPackageRowsInRange,
	collectVisiblePackageRows,
} from "@/app/_main/projects/manage/-bulk-update-range-selection";

type TestPackageRow = {
	id: string;
	selectable: boolean;
};

const packageRows: TestPackageRow[] = [
	{ id: "a", selectable: true },
	{ id: "b", selectable: false },
	{ id: "c", selectable: true },
	{ id: "d", selectable: true },
	{ id: "e", selectable: true },
];
const hiddenPackageRows = packageRows.slice(3);

describe("bulk update range selection", () => {
	test("uses visible search results and excludes collapsed hidden packages", () => {
		const visibleRows = collectVisiblePackageRows(
			packageRows,
			hiddenPackageRows,
			new Set(["a", "c", "d", "e"]),
			false,
		);

		expect(visibleRows.map((row) => row.id)).toEqual(["a", "c"]);
	});

	test("appends filtered hidden packages when their section is expanded", () => {
		const visibleRows = collectVisiblePackageRows(
			packageRows,
			hiddenPackageRows,
			new Set(["a", "c", "e"]),
			true,
		);

		expect(visibleRows.map((row) => row.id)).toEqual(["a", "c", "e"]);
	});

	test("selects an inclusive range in either direction and skips ineligible rows", () => {
		const canSelect = (row: TestPackageRow) => row.selectable;
		const forwardRange = collectPackageRowsInRange(
			packageRows,
			"a",
			"d",
			canSelect,
		);
		const reverseRange = collectPackageRowsInRange(
			packageRows,
			"d",
			"a",
			canSelect,
		);

		expect(forwardRange?.map((row) => row.id)).toEqual(["a", "c", "d"]);
		expect(reverseRange?.map((row) => row.id)).toEqual(["a", "c", "d"]);
	});

	test("falls back to a normal click when the anchor is not visible", () => {
		expect(
			collectPackageRowsInRange(packageRows.slice(0, 3), "d", "a", () => true),
		).toBeNull();
	});

	test("applies selection and deselection without disturbing outside rows", () => {
		const rangeRows = [packageRows[1], packageRows[2]];

		expect(applyPackageRangeSelection(["a"], rangeRows, true)).toEqual([
			"a",
			"b",
			"c",
		]);
		expect(
			applyPackageRangeSelection(["a", "b", "c", "e"], rangeRows, false),
		).toEqual(["a", "e"]);
	});
});
