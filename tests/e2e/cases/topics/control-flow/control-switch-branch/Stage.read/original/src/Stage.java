class Stage {
    static int read(String mode) {
        switch (mode) {
            case "ready":
                return 1;
            default:
                return 0;
        }
    }
}
