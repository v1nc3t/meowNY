package com.meowny.server.security;

import com.meowny.server.entity.User;
import com.meowny.server.exception.ResourceNotFoundException;
import com.meowny.server.repository.UserRepository;
import org.springframework.security.access.AccessDeniedException;
import org.springframework.security.core.Authentication;
import org.springframework.security.core.context.SecurityContextHolder;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

@Service
public class CurrentUserService {

    private final UserRepository userRepository;

    public CurrentUserService(UserRepository userRepository) {
        this.userRepository = userRepository;
    }

    @Transactional(readOnly = true)
    public User getCurrentUser() {
        Authentication authentication = SecurityContextHolder.getContext().getAuthentication();
        if (authentication == null || !authentication.isAuthenticated()) {
            throw new AccessDeniedException("Forbidden");
        }

        String username = authentication.getName();
        if (username == null || "anonymousUser".equals(username)) {
            throw new AccessDeniedException("Forbidden");
        }

        return userRepository.findUserByUsername(username)
                .orElseThrow(() -> new AccessDeniedException("Forbidden"));
    }

    public void requireOwnedByCurrentUser(Long resourceUserId) {
        if (!getCurrentUser().getId().equals(resourceUserId)) {
            throw new ResourceNotFoundException();
        }
    }
}
