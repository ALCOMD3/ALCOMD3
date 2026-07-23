type PackageRowIdentifier = {
	id: string;
};

export function collectVisiblePackageRows<Row extends PackageRowIdentifier>(
	packageRows: readonly Row[],
	hiddenPackageRows: readonly Row[],
	filteredPackageIds: ReadonlySet<string>,
	showHiddenPackages: boolean,
): Row[] {
	const hiddenPackageIds = new Set(hiddenPackageRows.map((row) => row.id));
	const visibleRows = packageRows.filter(
		(row) => !hiddenPackageIds.has(row.id) && filteredPackageIds.has(row.id),
	);

	if (showHiddenPackages) {
		visibleRows.push(
			...hiddenPackageRows.filter((row) => filteredPackageIds.has(row.id)),
		);
	}

	return visibleRows;
}

export function collectPackageRowsInRange<Row extends PackageRowIdentifier>(
	visiblePackageRows: readonly Row[],
	anchorPackageId: string,
	targetPackageId: string,
	canSelect: (row: Row) => boolean,
): Row[] | null {
	const anchorIndex = visiblePackageRows.findIndex(
		(row) => row.id === anchorPackageId,
	);
	const targetIndex = visiblePackageRows.findIndex(
		(row) => row.id === targetPackageId,
	);
	if (anchorIndex === -1 || targetIndex === -1) return null;

	const start = Math.min(anchorIndex, targetIndex);
	const end = Math.max(anchorIndex, targetIndex);
	return visiblePackageRows.slice(start, end + 1).filter(canSelect);
}

export function applyPackageRangeSelection(
	selectedPackageIds: readonly string[],
	rangePackageRows: readonly PackageRowIdentifier[],
	selected: boolean,
): string[] {
	const nextSelectedPackageIds = new Set(selectedPackageIds);
	for (const row of rangePackageRows) {
		if (selected) {
			nextSelectedPackageIds.add(row.id);
		} else {
			nextSelectedPackageIds.delete(row.id);
		}
	}
	return [...nextSelectedPackageIds];
}
