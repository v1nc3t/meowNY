package com.meowny.server.repository;

import com.meowny.server.entity.Budget;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;

import java.time.LocalDate;
import java.util.List;
import java.util.Optional;

public interface BudgetRepository extends JpaRepository<Budget, Long> {

    Optional<Budget> findByUserIdAndCategoryIdAndEffectiveFrom(Long userId, Long categoryId, LocalDate effectiveFrom);

    @Query("""
            SELECT b FROM Budget b
            WHERE b.user.id = :userId
              AND b.scope = com.meowny.server.entity.BudgetScope.GLOBAL
              AND b.category IS NULL
              AND b.effectiveFrom = :effectiveFrom
            """)
    Optional<Budget> findGlobalByUserIdAndEffectiveFrom(
            @Param("userId") Long userId,
            @Param("effectiveFrom") LocalDate effectiveFrom
    );

    List<Budget> findByUserIdAndEffectiveFrom(Long userId, LocalDate effectiveFrom);

    void deleteByUserIdAndCategoryId(Long userId, Long categoryId);

    boolean existsByCategoryId(Long categoryId);
}
