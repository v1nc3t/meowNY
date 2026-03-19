package commons;

import org.junit.jupiter.api.*;

import static org.junit.jupiter.api.Assertions.*;

public class ExpenseCategoryTest {

    private ExpenseCategory testExpenseCategory1;
    private ExpenseCategory testExpenseCategory2;
    private ExpenseCategory testExpenseCategory3;

    @BeforeEach
    void setUp() {
        testExpenseCategory1 = new ExpenseCategory("food");
        testExpenseCategory2 = new ExpenseCategory("car");
        testExpenseCategory3 = new ExpenseCategory("car");
    }

    @Test
    void invalidConstructorTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testExpenseCategory1 = new ExpenseCategory(null),
                "invalid category name"
        );
    }

    @Test
    void setIdTest() {
        Long id = 1L;
        testExpenseCategory1.setId(id);
        assertEquals(id, testExpenseCategory1.getId());
    }

    @Test
    void setValidCategoryNameTest() {
        String newCategory = "water";
        testExpenseCategory1.setCategoryName(newCategory);
        assertEquals(newCategory, testExpenseCategory1.getCategoryName());
    }

    @Test
    void setInvalidCategoryNameTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testExpenseCategory1.setCategoryName(null),
                "invalid category name"
        );
    }

    @Test
    void notEqualsTest() {
        assertNotEquals(testExpenseCategory1, testExpenseCategory2);
    }

    @Test
    void equalsTest() {
        assertEquals(testExpenseCategory3, testExpenseCategory2);
    }

    @Test
    void hashCodeDiffTest() {
        assertNotEquals(testExpenseCategory1.hashCode(), testExpenseCategory2.hashCode());
    }

    @Test
    void hashCodeSameTest() {
        assertEquals(testExpenseCategory3.hashCode(), testExpenseCategory2.hashCode());
    }

}
