local M = {}
M.__index = M

function M.new(concurrency)
  return setmetatable({
    concurrency = concurrency or math.huge,
    pending = 0,
    jobs = {},
  }, M)
end

function M:add(job)
  self.jobs[#self.jobs + 1] = job
  self:_run()
end

function M:_run()
  if self.pending >= self.concurrency then return end
  if #self.jobs == 0 then return end

  local job = table.remove(self.jobs, 1)
  self.pending = self.pending + 1

  local self_ref = self
  job(function()
    self_ref.pending = self_ref.pending - 1
    self_ref:_run()
  end)
end

return M
