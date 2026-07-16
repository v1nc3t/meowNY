package com.meowny.server.repository;

import com.meowny.commons.entity.Transaction;
import org.springframework.data.domain.Page;
import org.springframework.data.domain.Pageable;
import org.springframework.data.jpa.repository.JpaRepository;

public interface TransactionRepository extends JpaRepository<Transaction, Long> {

    boolean existsByCategoryId(Long categoryId);

    boolean existsByRecurringTransactionId(Long recurringTransactionId);

    Page<Transaction> findByUserId(Long userId, Pageable pageable);

    Page<Transaction> findByUserIdAndCategoryId(Long userId, Long categoryId, Pageable pageable);
}
