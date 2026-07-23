import { readFile } from "node:fs/promises";
import { relative } from "node:path";
import { fileURLToPath } from "node:url";
import { defineCollection } from "astro:content";
import type { Loader } from "astro/loaders";
import { z } from "astro/zod";
import { supportedRouteLocales, type RouteLocale } from "@/data/i18n";

type McpDocSource = {
    lang: RouteLocale;
    path: string;
};

const mcpDocSources: McpDocSource[] = [
    {
        lang: "zh-cn",
        path: "../../docs/mcp/mcp.zh-CN.md",
    },
    {
        lang: "zh-tw",
        path: "../../docs/mcp/mcp.zh-TW.md",
    },
    {
        lang: "ja-jp",
        path: "../../docs/mcp/mcp.ja.md",
    },
    {
        lang: "en-us",
        path: "../../docs/mcp.md",
    },
];

function removeSourceLanguageLine(markdown: string): string {
    return markdown.replace(
        /^(# .+\r?\n)\r?\n(?:Languages|语言|語言|言語):[^\r\n]*(?:\r?\n){1,2}/u,
        "$1\n",
    );
}

function toPosixPath(path: string): string {
    return path.replaceAll("\\", "/");
}

function createMcpDocsLoader(): Loader {
    return {
        name: "alcomd3-mcp-docs-loader",
        async load({ config, generateDigest, parseData, renderMarkdown, store, watcher }) {
            store.clear();

            for (const source of mcpDocSources) {
                const fileUrl = new URL(source.path, import.meta.url);
                const filePath = fileURLToPath(fileUrl);
                const rawMarkdown = await readFile(filePath, "utf8");
                const body = removeSourceLanguageLine(rawMarkdown);
                const data = await parseData({
                    id: source.lang,
                    data: {
                        lang: source.lang,
                    },
                    filePath,
                });

                store.set({
                    id: source.lang,
                    data,
                    body,
                    digest: generateDigest(body),
                    filePath: toPosixPath(relative(fileURLToPath(config.root), filePath)),
                    rendered: await renderMarkdown(body, {
                        fileURL: fileUrl,
                    }),
                });

                watcher?.add(filePath);
            }
        },
    };
}

const mcpDocs = defineCollection({
    loader: createMcpDocsLoader(),
    schema: z.object({
        lang: z.enum(supportedRouteLocales),
    }),
});

export const collections = {
    mcpDocs,
};
