class SwitchMode {
    static int classify(boolean stop) {
        int result = 0;
        switch (1) {
            case 1 -> {
                if (stop) { result = 8; break; }
                result = 9;
            }
            default -> {}
        }
        return result;
    }
}
