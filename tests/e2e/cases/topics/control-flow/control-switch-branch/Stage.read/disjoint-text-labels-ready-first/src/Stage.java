class Stage {
    static int read(String mode) {
        switch (mode) {
            case "ready":
                return 1;
            case "paused":
                return 2;
            default:
                return 0;
        }
    }
}
