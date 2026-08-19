package com.meowny.server.controller;

import com.meowny.server.dto.user.UpdateUserRequest;
import com.meowny.server.dto.user.UserResponse;
import com.meowny.server.service.UserService;
import jakarta.validation.Valid;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api/v1/users")
public class UserController {

    private final UserService userService;

    public UserController(UserService userService) {
        this.userService = userService;
    }

    @GetMapping("/me")
    public ResponseEntity<UserResponse> getCurrentUser() {
        return ResponseEntity.ok(userService.getCurrentUserProfile());
    }

    @PutMapping("/me")
    public ResponseEntity<UserResponse> updateCurrentUser(@Valid @RequestBody UpdateUserRequest request) {
        return ResponseEntity.ok(userService.updateCurrentUser(request));
    }

    @DeleteMapping("/me")
    public ResponseEntity<Void> deleteCurrentUser() {
        userService.deleteCurrentUser();
        return ResponseEntity.noContent().build();
    }
}
