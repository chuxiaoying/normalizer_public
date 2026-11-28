class SwitchMode {
    static int classify(boolean stop) {
        int result = 0;
        switch (1) {
            case 1:
                result = 10;
                if (stop) { break; }
            case 2:
                result += 5;
                break;
            default:
                result = 99;
        }
        return result;
    }
}
