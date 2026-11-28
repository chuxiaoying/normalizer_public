class SwitchMode {
    static int classify() {
        int result = 0;
        switch (1) {
            case 1 -> {
                for (int i = 0; i < 3; i++) {
                    result += 1;
                    if (i == 1) break;
                }
                result += 10;
            }
            default -> {}
        }
        return result;
    }
}
