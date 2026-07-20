// SPDX-License-Identifier: MIT
pragma solidity 0.8.35;

contract MockERC20 {
    mapping(address account => uint256) public balanceOf;
    mapping(address owner => mapping(address spender => uint256)) public allowance;

    function mint(address account, uint256 amount) external {
        balanceOf[account] += amount;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transferFrom(address owner, address recipient, uint256 amount) external returns (bool) {
        uint256 approved = allowance[owner][msg.sender];
        if (approved < amount || balanceOf[owner] < amount) return false;

        allowance[owner][msg.sender] = approved - amount;
        balanceOf[owner] -= amount;
        balanceOf[recipient] += amount;
        return true;
    }
}
