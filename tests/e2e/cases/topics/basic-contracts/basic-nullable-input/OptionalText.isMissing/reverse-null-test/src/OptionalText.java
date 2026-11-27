import normalizer.intrinsics.N;

class OptionalText {
    static boolean isMissing(@N.Nullable String value) {
        return value != null;
    }
}
