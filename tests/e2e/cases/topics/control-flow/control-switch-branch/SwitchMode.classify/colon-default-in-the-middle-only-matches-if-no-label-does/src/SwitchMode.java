class SwitchMode {
    static int classify(int code) {
        int result = 0;
        switch (code) {
            case 1: result = 10;
            default: result += 1;
            case 2: result += 5; break;
        }
        return result;
    }
}
