import {existsSync} from 'node:fs';
import {dirname, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawn} from 'node:child_process';

const ideaDir = dirname(fileURLToPath(import.meta.url));
const userDataDir = resolve(ideaDir, 'chrome-user-data');
const url = getViteUrl();

if (!url) {
    console.error('[open-chrome-dev] 未收到 Vite URL。');
    process.exit(1);
}

const chrome = resolveChromeExecutable();
const devServerPid = process.ppid;
const child = spawn(chrome, [
    '--new-window',
    `--user-data-dir=${userDataDir}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-background-mode',
    url
], {
    detached: true,
    stdio: 'ignore'
});

child.once('error', (error) => {
    console.error(`[open-chrome-dev] 无法启动 Chrome：${error.message}`);
    process.exit(1);
});

child.once('exit', () => {
    stopDevServer(devServerPid);
});

function resolveChromeExecutable() {
    if (process.platform !== 'win32') {
        return 'google-chrome';
    }

    const candidates = [
        process.env.LOCALAPPDATA && `${process.env.LOCALAPPDATA}\\Google\\Chrome\\Application\\chrome.exe`,
        process.env.PROGRAMFILES && `${process.env.PROGRAMFILES}\\Google\\Chrome\\Application\\chrome.exe`,
        process.env['PROGRAMFILES(X86)'] && `${process.env['PROGRAMFILES(X86)']}\\Google\\Chrome\\Application\\chrome.exe`
    ].filter(Boolean);

    return candidates.find((candidate) => existsSync(candidate)) ?? 'chrome.exe';
}

function getViteUrl() {
    for (let index = process.argv.length - 1; index >= 0; index -= 1) {
        const argument = process.argv[index];

        if (/^https?:\/\//.test(argument)) {
            return argument;
        }
    }

    return '';
}

function stopDevServer(pid) {
    if (!pid || pid <= 0) {
        process.exit(0);
    }

    try {
        process.kill(pid, 'SIGINT');
    } catch {
        process.exit(0);
    }

    setTimeout(() => {
        try {
            process.kill(pid, 'SIGTERM');
        } catch {}

        process.exit(0);
    }, 1500);
}
