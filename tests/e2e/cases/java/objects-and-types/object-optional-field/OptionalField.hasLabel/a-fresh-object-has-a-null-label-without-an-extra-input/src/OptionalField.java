import normalizer.intrinsics.N;

class Profile {
    @N.Nullable
    String label;
}

class OptionalField {
    static int hasLabel(Profile profile) {
        Profile fresh = new Profile(); return fresh.label == null ? 0 : 1;
    }
}
