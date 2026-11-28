class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case (2 - 1) * 1 -> { return 10; }
            case 1 + 1 -> { return 20; }
            default -> { return 0; }
        }
    }
}
