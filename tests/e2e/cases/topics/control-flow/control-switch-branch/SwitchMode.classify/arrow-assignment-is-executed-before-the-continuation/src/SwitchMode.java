class SwitchMode {
    static int classify(int code) {
        int result = 0;
        switch (code) {
            case 1 -> result = 10;
            case 2 -> { result = 20; }
            default -> {}
        }
        return result;
    }
}
