import normalizer.intrinsics.N;

class Stage {
    static int read(@N.Nullable String mode) {
        switch (mode) {
            case "ready":
                return 1;
            default:
                return 0;
        }
    }
}
