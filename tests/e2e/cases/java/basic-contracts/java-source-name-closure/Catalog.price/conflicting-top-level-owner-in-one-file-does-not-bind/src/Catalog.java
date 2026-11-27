class Catalog {
    static int price() { return Price.price(); }
}
class Price { static int price() { return 99; } }
class Price {
    static int price() { return 12; }
}
class OtherPrice {
    static int price() { return 99; }
}
