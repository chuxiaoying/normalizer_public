import java.math.BigDecimal;
import normalizer.intrinsics.N;

class DecimalPrice {
    static BigDecimal roundPrice(BigDecimal value) {
        BigDecimal exact; /* @normalizer-ir exact = 2.345000000000000000000000000001 */ return N.round(exact, 2, N.HALF_EVEN);
    }
}
