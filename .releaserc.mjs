// Releases the Android app. The Worker is not versioned here: each API version deploys from its own
// branch on every push.
export default {
    branches: ['v3'],
    tagFormat: 'android-v${version}',
    plugins: [
        './.github/scripts/android-release.mjs',
        [
            '@semantic-release/git',
            {
                assets: ['android-app/app/build.gradle.kts'],
                message:
                    'chore(android): release ${nextRelease.version} [skip ci]'
            }
        ],
        [
            '@semantic-release/github',
            {
                releaseNameTemplate: 'Sniff Android <%= nextRelease.version %>',
                assets: [
                    {
                        path: 'android-app/app/build/outputs/apk/release/sniff-android-*.apk'
                    }
                ],
                successComment: false,
                failComment: false,
                releasedLabels: false
            }
        ]
    ]
};
