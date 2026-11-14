// The formatters run on the whole project, so these tasks take no file names. lint-staged adds the
// formatted versions of the staged files to the commit.
export default {
    "*.rs": () => "cargo fmt --all",
    "android-app/**/*.{kt,kts}": () => "./android-app/gradlew -p android-app spotlessApply",
    "*": "bun fmt --ignore-unknown",
};
