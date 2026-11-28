class Stage {
    static int read(String mode) {
        switch (mode) {
            case "paused":
                return 2;
            case "ready":
                return 1;
            default:
                return 0;
        }
    }
}
