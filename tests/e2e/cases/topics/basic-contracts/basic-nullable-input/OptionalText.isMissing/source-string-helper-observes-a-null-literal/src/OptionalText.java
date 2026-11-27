import normalizer.intrinsics.N;

class OptionalText {
    static boolean helper(@N.Nullable String text) {
        return text == null;
    }
    static boolean isMissing(@N.Nullable String value) {
        return helper(null);
    }
}
