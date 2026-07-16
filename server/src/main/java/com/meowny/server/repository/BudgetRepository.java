package com.meowny.server.repository;

import com.meowny.commons.entity.Budget;
import org.springframework.data.jpa.repository.JpaRepository;

import java.util.List;
import java.util.Optional;

public interface BudgetRepository extends JpaRepository<Budget, Long> {

    Optional<Budget> findByUserIdAndCategoryIdAndMonthAndYear(Long userId, Long categoryId, Integer month, Integer year);

    List<Budget> findByUserIdAndYearAndMonth(Long userId, Integer year, Integer month);

    void deleteByUserIdAndCategoryId(Long userId, Long categoryId);

    boolean existsByCategoryId(Long categoryId);
}
