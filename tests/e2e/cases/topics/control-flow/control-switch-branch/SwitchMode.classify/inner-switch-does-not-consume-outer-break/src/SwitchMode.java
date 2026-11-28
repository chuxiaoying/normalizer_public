class SwitchMode {
    static int classify(boolean stop) {
        int result = 0;
        switch (1) {
            case 1:
                if (stop) { result = 10; break; }
                switch (2) {
                    case 2 -> { { result = 20; break; } }
                    default -> { result = 30; }
                }
                result += 1;
                break;
            default:
                result = 99;
        }
        return result;
    }
}
