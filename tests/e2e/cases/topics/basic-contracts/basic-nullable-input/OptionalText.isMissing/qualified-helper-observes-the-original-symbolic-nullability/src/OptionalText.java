import normalizer.intrinsics.N;

class OptionalText {
    static boolean probe(@N.Nullable java.lang.String text) {
        return text == null;
    }
    static boolean isMissing(@N.Nullable String value) {
        return OptionalText.probe(value);
    }
}
