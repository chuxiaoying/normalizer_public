import normalizer.intrinsics.N;

class Profile {
    @N.Nullable
    String label;
}

class OptionalField {
    static Profile hasLabel(Profile profile) {
        Profile out = new Profile(); out.label = ""; return out;
    }
}
