import normalizer.intrinsics.N;

class Profile {
    @N.Nullable
    String label; boolean enabled = true; byte debit = -128; short credit = 32767;
}

class OptionalField {
    static int hasLabel(Profile profile) {
        Profile fresh = new Profile(); return fresh.enabled && fresh.label == null ? fresh.debit + fresh.credit : 0;
    }
}
