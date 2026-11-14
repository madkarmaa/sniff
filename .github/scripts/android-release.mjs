import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { analyzeCommits as analyzeAllCommits } from "@semantic-release/commit-analyzer";
import { generateNotes as generateAllNotes } from "@semantic-release/release-notes-generator";

const APP_DIRECTORY = "android-app/";
const BUILD_FILE = "android-app/app/build.gradle.kts";
const APK_DIRECTORY = "android-app/app/build/outputs/apk/release";
const REQUIRED_VARIABLES = [
    "ANDROID_HOME",
    "ANDROID_BUILD_TOOLS",
    "ANDROID_KEYSTORE_BASE64",
    "ANDROID_KEYSTORE_PASSWORD",
    "ANDROID_KEY_ALIAS",
    "ANDROID_KEY_PASSWORD",
];
// The largest versionCode Google Play accepts.
const MAX_VERSION_CODE = 2_100_000_000;

const apkPath = (version) => `${APK_DIRECTORY}/sniff-android-${version}.apk`;

const touchesApp = (cwd, hash) =>
    execFileSync("git", ["diff-tree", "--no-commit-id", "--name-only", "-r", "--root", hash], {
        cwd,
        encoding: "utf8",
    })
        .split("\n")
        .some((path) => path.startsWith(APP_DIRECTORY));

// The Worker shares this history but not the app's version, so only commits that change the app
// decide the release and appear in its notes.
const withAppCommits = (context) => ({
    ...context,
    commits: context.commits.filter((commit) => touchesApp(context.cwd, commit.hash)),
});

export const analyzeCommits = (pluginConfig, context) =>
    analyzeAllCommits(pluginConfig, withAppCommits(context));

export const generateNotes = (pluginConfig, context) =>
    generateAllNotes(pluginConfig, withAppCommits(context));

// Failing here, before anything is tagged, keeps a release from going out without its APK.
export const verifyConditions = (_, { env }) => {
    const missing = REQUIRED_VARIABLES.filter((name) => !env[name]);
    if (missing.length > 0) throw new Error(`Missing environment variables: ${missing.join(", ")}`);
};

export const prepare = (_, { cwd, env, nextRelease, logger }) => {
    const versionCode = setAppVersion(cwd, nextRelease.version);
    logger.log(`Building Sniff Android ${nextRelease.version} (version code ${versionCode})`);

    execFileSync(
        "./android-app/gradlew",
        ["-p", "android-app", "--no-daemon", ":app:lintRelease", ":app:assembleRelease"],
        { cwd, env, stdio: "inherit" },
    );
    signApk(cwd, env, nextRelease.version);
};

const setAppVersion = (cwd, version) => {
    const path = join(cwd, BUILD_FILE);
    const source = readFileSync(path, "utf8");
    const versionCode = Number(source.match(/^\s*versionCode = (\d+)$/m)?.[1]) + 1;
    if (!Number.isSafeInteger(versionCode) || versionCode > MAX_VERSION_CODE) {
        throw new Error(`${BUILD_FILE} has no usable versionCode`);
    }

    writeFileSync(
        path,
        source
            .replace(/^(\s*versionCode = )\d+$/m, `$1${versionCode}`)
            .replace(/^(\s*versionName = )"[^"]*"$/m, `$1"${version}"`),
    );
    return versionCode;
};

const signApk = (cwd, env, version) => {
    const tools = `${env.ANDROID_HOME}/build-tools/${env.ANDROID_BUILD_TOOLS}`;
    const unsigned = join(cwd, APK_DIRECTORY, "app-release-unsigned.apk");
    const signed = join(cwd, apkPath(version));
    const temporary = mkdtempSync(join(tmpdir(), "sniff-release-"));
    const keystore = join(temporary, "release.keystore");
    const aligned = join(temporary, "aligned.apk");
    const run = (tool, args) => execFileSync(`${tools}/${tool}`, args, { env, stdio: "inherit" });

    try {
        writeFileSync(keystore, Buffer.from(env.ANDROID_KEYSTORE_BASE64, "base64"), {
            mode: 0o600,
        });
        run("zipalign", ["-P", "16", "-f", "4", unsigned, aligned]);
        // The passwords stay in the environment, so they never show in a process list.
        run("apksigner", [
            "sign",
            ...["--ks", keystore, "--ks-key-alias", env.ANDROID_KEY_ALIAS],
            ...["--ks-pass", "env:ANDROID_KEYSTORE_PASSWORD"],
            ...["--key-pass", "env:ANDROID_KEY_PASSWORD"],
            ...["--v1-signing-enabled", "false", "--v2-signing-enabled", "true"],
            ...["--v3-signing-enabled", "true", "--v4-signing-enabled", "false"],
            ...["--out", signed, aligned],
        ]);
        run("apksigner", ["verify", "--verbose", signed]);
        run("zipalign", ["-c", "-P", "16", "4", signed]);
    } finally {
        rmSync(temporary, { recursive: true, force: true });
    }
};
