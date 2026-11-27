import normalizer.intrinsics.N;

class Profile {
    @N.Nullable
    java.lang.String label;
}

class OptionalField {
    static int hasLabel(Profile profile) {
        return profile.label == null ? 0 : 1;
    }
}
