package commons;

import org.junit.jupiter.api.*;

import static org.junit.jupiter.api.Assertions.*;

public class IncomeCategoryTest {

    private IncomeCategory testIncomeCategory1;
    private IncomeCategory testIncomeCategory2;
    private IncomeCategory testIncomeCategory3;

    @BeforeEach
    void setUp() {
        testIncomeCategory1 = new IncomeCategory("rent");
        testIncomeCategory2 = new IncomeCategory("car");
        testIncomeCategory3 = new IncomeCategory("car");
    }

    @Test
    void invalidConstructorTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testIncomeCategory1 = new IncomeCategory(null),
                "invalid category name"
        );
    }

    @Test
    void setIdTest() {
        Long id = 1L;
        testIncomeCategory1.setId(id);
        assertEquals(id, testIncomeCategory1.getId());
    }

    @Test
    void setValidCategoryNameTest() {
        String newCategory = "hustle";
        testIncomeCategory1.setCategoryName(newCategory);
        assertEquals(newCategory, testIncomeCategory1.getCategoryName());
    }

    @Test
    void setInvalidCategoryNameTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testIncomeCategory1.setCategoryName(null),
                "invalid category name"
        );
    }

    @Test
    void notEqualsTest() {
        assertNotEquals(testIncomeCategory1, testIncomeCategory2);
    }

    @Test
    void equalsTest() {
        assertEquals(testIncomeCategory3, testIncomeCategory2);
    }

    @Test
    void hashCodeDiffTest() {
        assertNotEquals(testIncomeCategory1.hashCode(), testIncomeCategory2.hashCode());
    }

    @Test
    void hashCodeSameTest() {
        assertEquals(testIncomeCategory3.hashCode(), testIncomeCategory2.hashCode());
    }

}
