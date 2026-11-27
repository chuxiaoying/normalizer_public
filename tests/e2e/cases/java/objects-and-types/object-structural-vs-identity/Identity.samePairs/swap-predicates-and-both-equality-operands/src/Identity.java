class Box { int value; }
class Identity {
    static boolean samePairs(Box zebra, Box yellow, Box blue, Box amber) {
        return amber == blue && yellow == zebra;
    }
}
