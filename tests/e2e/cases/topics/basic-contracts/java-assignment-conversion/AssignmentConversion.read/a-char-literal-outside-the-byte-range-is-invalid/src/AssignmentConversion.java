class AssignmentConversion {
    static int read() {
        byte value = 'é';
        value += 99L;
        return value;
    }
}
