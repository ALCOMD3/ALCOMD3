import { existsSync, realpathSync } from "node:fs";
import path from "node:path";

export function canonicalizePath(input) {
	let existingAncestor = path.resolve(input);
	const missingSegments = [];

	while (!existsSync(existingAncestor)) {
		const ancestor = path.dirname(existingAncestor);
		if (ancestor === existingAncestor) {
			break;
		}
		missingSegments.unshift(path.basename(existingAncestor));
		existingAncestor = ancestor;
	}

	const canonicalAncestor = existsSync(existingAncestor)
		? realpathSync.native(existingAncestor)
		: existingAncestor;
	return path.resolve(canonicalAncestor, ...missingSegments);
}

export function isStrictChildPath(parent, child) {
	const relative = path.relative(
		canonicalizePath(parent),
		canonicalizePath(child),
	);
	return (
		relative.length > 0 &&
		!relative.startsWith(`..${path.sep}`) &&
		relative !== ".." &&
		!path.isAbsolute(relative)
	);
}
