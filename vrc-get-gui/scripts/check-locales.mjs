import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import JSON5 from "json5";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const localeDir = path.resolve(__dirname, "../locales");
const baseLocaleFile = "en.json5";

const localeFiles = (await readdir(localeDir))
	.filter((fileName) => fileName.endsWith(".json5"))
	.sort();

const translations = new Map();
let hasFailure = false;

for (const localeFile of localeFiles) {
	const localePath = path.join(localeDir, localeFile);
	const locale = JSON5.parse(await readFile(localePath, "utf8"));
	const translation = locale.translation;

	if (typeof translation !== "object" || translation == null) {
		console.error(`${localeFile}: missing translation object`);
		hasFailure = true;
		continue;
	}

	translations.set(localeFile, translation);
}

const baseTranslation = translations.get(baseLocaleFile);

if (baseTranslation == null) {
	console.error(`${baseLocaleFile}: base locale is missing`);
	process.exit(1);
}

const baseKeys = Object.keys(baseTranslation).sort();

for (const localeFile of localeFiles) {
	const translation = translations.get(localeFile);
	if (translation == null) continue;

	const missingKeys = baseKeys.filter((key) => !(key in translation));
	if (missingKeys.length > 0) {
		console.error(
			`${localeFile}: missing ${missingKeys.length} locale keys from ${baseLocaleFile}:`,
		);
		for (const key of missingKeys) {
			console.error(`  - ${key}`);
		}
		hasFailure = true;
	}
}

if (hasFailure) {
	process.exitCode = 1;
} else {
	console.log(
		`Locale coverage OK (${localeFiles.length} locales, ${baseKeys.length} keys).`,
	);
}
